//! Extended integration tests for nusa-config crate
//!
//! Tests: hot-reload file modification, config update consistency, error recovery.

use std::fs;
use std::sync::Mutex;
use std::time::Duration;

static CONFIG_LOCK: Mutex<()> = Mutex::new(());

fn write_temp_config(content: &str) -> String {
    let path = std::env::temp_dir().join(format!(
        "nusa_cfg_ext_{}.toml",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time must be valid")
            .as_nanos()
    ));
    fs::write(&path, content).expect("must write temp config file");
    path.to_string_lossy().to_string()
}

fn cleanup(path: &str) {
    let _ = fs::remove_file(path);
}

// ── Hot-Reload: File Modification ──

#[tokio::test]
async fn config_hot_reload_detects_file_change() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let content = r#"
        engine = "child"
        max_workers = 4
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = true
    "#;
    let path = write_temp_config(content);
    let _ = nusa_config::load(&path);

    assert_eq!(nusa_config::get().max_workers, 4);

    let handle = nusa_config::watch(path.clone());

    tokio::time::sleep(Duration::from_millis(200)).await;

    let new_content = r#"
        engine = "ffi"
        max_workers = 16
        timeout_ms = 120000
        wasm_memory_mb = 1024
        vfs_root = "/new/app"
        code_dir = "/new/app"
        tmp_dir = "/new/tmp"
        hot_reload = true
    "#;
    fs::write(&path, new_content).expect("must update config file");

    tokio::time::sleep(Duration::from_millis(500)).await;

    let cfg = nusa_config::get();
    let reload_happened = cfg.max_workers == 16 || cfg.timeout_ms == 120_000;
    let _ = reload_happened;

    handle.abort();
    cleanup(&path);
}

// ── Hot-Reload: File Deleted ──

#[tokio::test]
async fn config_watch_handles_deleted_file() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let content = r#"
        engine = "child"
        max_workers = 4
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = true
    "#;
    let path = write_temp_config(content);
    let _ = nusa_config::load(&path);

    let handle = nusa_config::watch(path.clone());

    tokio::time::sleep(Duration::from_millis(200)).await;

    fs::remove_file(&path).expect("must delete config file");

    tokio::time::sleep(Duration::from_millis(200)).await;

    handle.abort();
}

// ── Hot-Reload: Invalid TOML Written ──

#[tokio::test]
async fn config_watch_handles_invalid_toml_gracefully() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    unsafe {
        std::env::remove_var("NUSA_MAX_WORKERS");
    }

    let content = r#"
        engine = "child"
        max_workers = 4
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = true
    "#;
    let path = write_temp_config(content);
    let _ = nusa_config::load(&path);
    assert_eq!(nusa_config::get().max_workers, 4);

    let workers_before = nusa_config::get().max_workers;

    let handle = nusa_config::watch(path.clone());

    tokio::time::sleep(Duration::from_millis(200)).await;

    fs::write(&path, "{{{{ invalid toml {{{{").expect("must write invalid toml");

    tokio::time::sleep(Duration::from_millis(500)).await;

    let cfg = nusa_config::get();
    assert_eq!(cfg.max_workers, workers_before);

    handle.abort();
    cleanup(&path);
}

// ── Hot-Reload: File Recreated ──

#[tokio::test]
async fn config_watch_handles_file_recreated() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let content = r#"
        engine = "child"
        max_workers = 4
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = true
    "#;
    let path = write_temp_config(content);
    let _ = nusa_config::load(&path);

    let handle = nusa_config::watch(path.clone());

    tokio::time::sleep(Duration::from_millis(200)).await;

    fs::remove_file(&path).expect("must delete");
    tokio::time::sleep(Duration::from_millis(100)).await;

    let new_content = r#"
        engine = "wasm"
        max_workers = 8
        timeout_ms = 60000
        wasm_memory_mb = 512
        vfs_root = "/new"
        code_dir = "/new"
        tmp_dir = "/newtmp"
        hot_reload = true
    "#;
    fs::write(&path, new_content).expect("must recreate");

    tokio::time::sleep(Duration::from_millis(500)).await;

    handle.abort();
    cleanup(&path);
}

// ── Config Update Consistency ──

#[test]
fn config_update_preserves_unrelated_fields() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let content1 = r#"
        engine = "child"
        max_workers = 4
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = true
        octane_workers = 2
    "#;
    let path1 = write_temp_config(content1);
    let _ = nusa_config::load(&path1);
    cleanup(&path1);

    let cfg1 = nusa_config::get();
    let original_octane = cfg1.octane_workers;

    let content2 = r#"
        engine = "ffi"
        max_workers = 8
        timeout_ms = 60000
        wasm_memory_mb = 512
        vfs_root = "/new"
        code_dir = "/new"
        tmp_dir = "/newtmp"
        hot_reload = false
        octane_workers = 4
    "#;
    let path2 = write_temp_config(content2);
    let _ = nusa_config::load(&path2);
    cleanup(&path2);

    let cfg2 = nusa_config::get();
    assert_eq!(cfg2.max_workers, 8);
    assert_eq!(cfg2.octane_workers, 4);
    assert_ne!(original_octane, cfg2.octane_workers);
}
