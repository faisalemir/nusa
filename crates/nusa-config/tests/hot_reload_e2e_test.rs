//! S11: Hot reload E2E tests.
//!
//! Covers real stack validation: file change detection, ArcSwap swap, and in-flight isolation.
//! Authoritative gate: `just podman-test-pkg nusa-config` (Alpine).

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use nusa_config::{get, load_sources};

fn temp_toml(name: &str, content: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("nusa_hot_reload_e2e");
    fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join(name);
    fs::write(&path, content).expect("write temp toml");
    path
}

fn cleanup_temp() {
    let dir = std::env::temp_dir().join("nusa_hot_reload_e2e");
    let _ = fs::remove_dir_all(&dir);
}

// === File Modification Detection ===

#[tokio::test]
async fn hot_reload_file_modification_detected() {
    cleanup_temp();
    let toml = temp_toml(
        "reload.toml",
        r#"
engine = "child"
max_workers = 4
bind = "0.0.0.0:8080"
"#,
    );
    let path = toml.to_str().unwrap();

    // Initial load
    load_sources(Some(path)).expect("initial load");
    assert_eq!(get().max_workers, 4);

    // Modify file
    fs::write(
        &toml,
        r#"
engine = "child"
max_workers = 16
bind = "0.0.0.0:8080"
"#,
    )
    .expect("write modified");

    // Reload
    load_sources(Some(path)).expect("reload");
    assert_eq!(get().max_workers, 16);

    cleanup_temp();
}

// === Invalid TOML During Watch (Graceful) ===

#[tokio::test]
async fn hot_reload_invalid_toml_preserves_old_config() {
    cleanup_temp();
    let toml = temp_toml(
        "invalid_reload.toml",
        r#"
engine = "child"
max_workers = 4
bind = "0.0.0.0:8080"
"#,
    );
    let path = toml.to_str().unwrap();

    // Initial load
    load_sources(Some(path)).expect("initial load");
    let old_workers = get().max_workers;
    assert_eq!(old_workers, 4);

    // Write invalid TOML
    fs::write(&toml, "this is not valid toml {{{").expect("write invalid");

    // Reload should fail, old config preserved
    let result = load_sources(Some(path));
    assert!(result.is_err(), "invalid TOML should fail to load");
    let cfg = get();
    assert_eq!(cfg.max_workers, old_workers, "old config must be preserved");

    cleanup_temp();
}

// === File Deleted and Recreated ===

#[tokio::test]
async fn hot_reload_file_deleted_and_recreated() {
    cleanup_temp();
    let toml = temp_toml(
        "delete_reload.toml",
        r#"
engine = "child"
max_workers = 8
bind = "0.0.0.0:8080"
"#,
    );
    let path = toml.to_str().unwrap();

    // Initial load
    load_sources(Some(path)).expect("initial load");
    assert_eq!(get().max_workers, 8);

    // Delete file
    fs::remove_file(&toml).expect("delete");

    // Load with nonexistent file should use defaults + env only
    // (load_sources handles missing file gracefully)
    let result = load_sources(Some(path));
    // May succeed (env-only) or fail depending on validation
    let _ = result;

    // Recreate file
    fs::write(
        &toml,
        r#"
engine = "ffi"
max_workers = 2
bind = "127.0.0.1:3000"
"#,
    )
    .expect("recreate");

    let result = load_sources(Some(path));
    assert!(result.is_ok(), "recreated file should load");
    assert_eq!(get().max_workers, 2);

    cleanup_temp();
}

// === In-Flight Request Isolation (ArcSwap) ===

#[tokio::test]
async fn hot_reload_in_flight_isolation() {
    cleanup_temp();
    let toml = temp_toml(
        "inflight.toml",
        r#"
engine = "child"
max_workers = 4
bind = "0.0.0.0:8080"
"#,
    );
    let path = toml.to_str().unwrap();

    load_sources(Some(path)).expect("initial load");

    // Simulate in-flight: capture Arc reference before reload
    let before_cfg = get();
    assert_eq!(before_cfg.max_workers, 4);

    // Reload with new value
    fs::write(
        &toml,
        r#"
engine = "child"
max_workers = 32
bind = "0.0.0.0:8080"
"#,
    )
    .expect("write modified");
    load_sources(Some(path)).expect("reload");

    // In-flight still sees old value (Arc reference)
    assert_eq!(before_cfg.max_workers, 4, "in-flight must see old config");

    // New get sees updated value
    let after_cfg = get();
    assert_eq!(after_cfg.max_workers, 32, "new get must see updated config");

    cleanup_temp();
}

// === Concurrent Access During Reload ===

#[tokio::test]
async fn hot_reload_concurrent_access_during_reload() {
    cleanup_temp();
    let toml = temp_toml(
        "concurrent_reload.toml",
        r#"
engine = "child"
max_workers = 4
bind = "0.0.0.0:8080"
"#,
    );
    let path = toml.to_str().unwrap();

    load_sources(Some(path)).expect("initial load");

    // Spawn concurrent readers
    let path_clone = path.to_string();
    let reader_handles: Vec<_> = (0..8)
        .map(|_| {
            tokio::spawn(async move {
                for _ in 0..100 {
                    let _ = get();
                    tokio::task::yield_now().await;
                }
            })
        })
        .collect();

    // Writer modifies file and reloads
    let writer_handle = tokio::spawn(async move {
        for i in 0..10 {
            fs::write(
                &toml,
                format!(
                    r#"
engine = "child"
max_workers = {}
bind = "0.0.0.0:8080"
"#,
                    4 + i
                ),
            )
            .expect("write");
            let _ = load_sources(Some(&path_clone));
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });

    // All readers must complete without panic
    for h in reader_handles {
        h.await.expect("reader panicked");
    }
    writer_handle.await.expect("writer panicked");

    // Final get must return valid config
    let cfg = get();
    assert!(cfg.max_workers >= 4);

    cleanup_temp();
}

// === Config Update Preserves Unrelated Fields ===

#[tokio::test]
async fn hot_reload_preserves_unrelated_fields() {
    cleanup_temp();
    let toml = temp_toml(
        "preserve.toml",
        r#"
engine = "child"
max_workers = 4
bind = "0.0.0.0:8080"
timeout_ms = 60000
"#,
    );
    let path = toml.to_str().unwrap();

    load_sources(Some(path)).expect("initial load");
    assert_eq!(get().timeout_ms, 60000);

    // Modify only max_workers
    fs::write(
        &toml,
        r#"
engine = "child"
max_workers = 16
bind = "0.0.0.0:8080"
timeout_ms = 60000
"#,
    )
    .expect("write modified");
    load_sources(Some(path)).expect("reload");

    let cfg = get();
    assert_eq!(cfg.max_workers, 16);
    assert_eq!(cfg.timeout_ms, 60000, "timeout_ms must be preserved");

    cleanup_temp();
}
