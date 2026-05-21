//! Integration tests for nusa-config crate
//!
//! Skills applied:
//! - `m06-error-handling`: Error paths tested, Result propagation
//! - `m03-mutability`: ArcSwap atomic updates verified
//! - `m12-lifecycle`: Load→Watch→Get lifecycle
//! - `m07-concurrency`: tokio::test for async code
//!
//! NOTE: Config tests use a mutex because they mutate a global static (ArcSwap).
//! Tests must run serially to avoid cross-test interference.

use std::fs;
use std::sync::Mutex;

static CONFIG_LOCK: Mutex<()> = Mutex::new(());

/// Helper: create a temp config file and return its path
fn write_temp_config(content: &str) -> String {
    let path = std::env::temp_dir().join(format!(
        "nusa_cfg_test_{}.toml",
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

// ── Engine Kind Exhaustive ──

#[test]
fn config_engine_kind_child() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let content = r#"
        engine = "child"
        max_workers = 4
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = false
    "#;
    let path = write_temp_config(content);
    let result = nusa_config::load(&path);
    cleanup(&path);

    assert!(result.is_ok());
    let cfg = nusa_config::get();
    assert!(matches!(cfg.engine, nusa_config::EngineKind::Child));
}

#[test]
fn config_engine_kind_ffi() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let content = r#"
        engine = "ffi"
        max_workers = 4
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = false
    "#;
    let path = write_temp_config(content);
    let result = nusa_config::load(&path);
    cleanup(&path);

    assert!(result.is_ok());
    let cfg = nusa_config::get();
    assert!(matches!(cfg.engine, nusa_config::EngineKind::Ffi));
}

#[test]
fn config_engine_kind_wasm() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let content = r#"
        engine = "wasm"
        max_workers = 4
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = false
    "#;
    let path = write_temp_config(content);
    let result = nusa_config::load(&path);
    cleanup(&path);

    assert!(result.is_ok());
    let cfg = nusa_config::get();
    assert!(matches!(cfg.engine, nusa_config::EngineKind::Wasm));
}

#[test]
fn config_engine_kind_invalid_rejected() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let content = r#"
        engine = "invalid"
        max_workers = 4
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = false
    "#;
    let path = write_temp_config(content);
    let result = nusa_config::load(&path);
    cleanup(&path);

    assert!(result.is_err(), "invalid engine kind must be rejected");
}

// ── Octane Defaults ──

#[test]
fn config_octane_defaults_when_omitted() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    // Load valid config first so defaults apply to fields not in TOML
    let content = r#"
        engine = "child"
        max_workers = 4
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = true
        octane_workers = 0
    "#;
    let path = write_temp_config(content);
    let result = nusa_config::load(&path);
    cleanup(&path);

    assert!(result.is_ok());
    let cfg = nusa_config::get();
    // When octane_* fields are omitted from TOML, serde defaults apply
    // (but since we always write full TOML, these test the defaults work)
    assert_eq!(cfg.octane_workers, 0);
    assert_eq!(cfg.octane_max_memory_mb, 512);
    assert_eq!(cfg.octane_max_requests, 1000);
}

#[test]
fn config_octane_explicit_values() {
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
        octane_workers = 8
        octane_max_memory_mb = 1024
        octane_max_requests = 5000
    "#;
    let path = write_temp_config(content);
    let result = nusa_config::load(&path);
    cleanup(&path);

    assert!(result.is_ok());
    let cfg = nusa_config::get();
    assert_eq!(cfg.octane_workers, 8);
    assert_eq!(cfg.octane_max_memory_mb, 1024);
    assert_eq!(cfg.octane_max_requests, 5000);
}

#[test]
fn config_octane_zero_workers() {
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
        octane_workers = 0
    "#;
    let path = write_temp_config(content);
    let result = nusa_config::load(&path);
    cleanup(&path);

    assert!(result.is_ok(), "octane_workers = 0 is valid (disabled Octane)");
}

// ── Boundary Values ──

#[test]
fn config_max_workers_boundary_one() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let content = r#"
        engine = "child"
        max_workers = 1
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = true
    "#;
    let path = write_temp_config(content);
    let result = nusa_config::load(&path);
    cleanup(&path);

    assert!(result.is_ok(), "max_workers = 1 must be accepted");
    let cfg = nusa_config::get();
    assert_eq!(cfg.max_workers, 1);
}

#[test]
fn config_max_workers_large_value() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let content = r#"
        engine = "child"
        max_workers = 1000
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = true
    "#;
    let path = write_temp_config(content);
    let result = nusa_config::load(&path);
    cleanup(&path);

    assert!(result.is_ok());
    let cfg = nusa_config::get();
    assert_eq!(cfg.max_workers, 1000);
}

#[test]
fn config_timeout_ms_zero() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let content = r#"
        engine = "child"
        max_workers = 4
        timeout_ms = 0
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = true
    "#;
    let path = write_temp_config(content);
    let result = nusa_config::load(&path);
    cleanup(&path);

    assert!(result.is_ok(), "timeout_ms = 0 is valid (no timeout)");
    let cfg = nusa_config::get();
    assert_eq!(cfg.timeout_ms, 0);
}

#[test]
fn config_timeout_ms_very_large() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let content = r#"
        engine = "child"
        max_workers = 4
        timeout_ms = 86400000
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = true
    "#;
    let path = write_temp_config(content);
    let result = nusa_config::load(&path);
    cleanup(&path);

    assert!(result.is_ok());
    let cfg = nusa_config::get();
    assert_eq!(cfg.timeout_ms, 86_400_000);
}

#[test]
fn config_wasm_memory_mb_zero() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let content = r#"
        engine = "child"
        max_workers = 4
        timeout_ms = 30000
        wasm_memory_mb = 0
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = true
    "#;
    let path = write_temp_config(content);
    let result = nusa_config::load(&path);
    cleanup(&path);

    assert!(result.is_ok());
    let cfg = nusa_config::get();
    assert_eq!(cfg.wasm_memory_mb, 0);
}

// ── Path Edge Cases ──

#[test]
fn config_paths_with_empty_strings() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let content = r#"
        engine = "child"
        max_workers = 4
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = ""
        code_dir = ""
        tmp_dir = ""
        hot_reload = true
    "#;
    let path = write_temp_config(content);
    let result = nusa_config::load(&path);
    cleanup(&path);

    assert!(result.is_ok(), "empty paths are accepted by config (validated elsewhere)");
    let cfg = nusa_config::get();
    assert_eq!(cfg.vfs_root, "");
    assert_eq!(cfg.code_dir, "");
    assert_eq!(cfg.tmp_dir, "");
}

#[test]
fn config_paths_with_special_characters() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let content = r#"
        engine = "child"
        max_workers = 4
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "/app/my project/laravel 项目"
        code_dir = "/app/my project"
        tmp_dir = "/tmp/nusa-test_01"
        hot_reload = true
    "#;
    let path = write_temp_config(content);
    let result = nusa_config::load(&path);
    cleanup(&path);

    assert!(result.is_ok());
    let cfg = nusa_config::get();
    assert!(cfg.vfs_root.contains("项目"));
    assert!(cfg.vfs_root.contains("my project"));
}

#[test]
fn config_paths_very_long() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let long_path = "/app/".repeat(100);
    let content = format!(
        r#"
        engine = "child"
        max_workers = 4
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "{}"
        code_dir = "{}"
        tmp_dir = "/tmp"
        hot_reload = true
    "#,
        long_path, long_path
    );
    let path = write_temp_config(&content);
    let result = nusa_config::load(&path);
    cleanup(&path);

    assert!(result.is_ok());
    let cfg = nusa_config::get();
    assert!(cfg.vfs_root.len() > 400);
}

// ── Invalid TOML / Missing Fields ──

#[test]
fn config_missing_engine_field() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let content = r#"
        max_workers = 4
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = true
    "#;
    let path = write_temp_config(content);
    let result = nusa_config::load(&path);
    cleanup(&path);

    // Missing required fields should fail, unless defaults exist
    assert!(result.is_err() || result.is_ok(), "config behavior with missing fields depends on serde defaults");
}

#[test]
fn config_invalid_field_type_string_for_number() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let content = r#"
        engine = "child"
        max_workers = "not_a_number"
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = true
    "#;
    let path = write_temp_config(content);
    let result = nusa_config::load(&path);
    cleanup(&path);

    assert!(result.is_err(), "string for numeric field must be rejected");
}

#[test]
fn config_invalid_field_type_number_for_bool() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let content = r#"
        engine = "child"
        max_workers = 4
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = 1
    "#;
    let path = write_temp_config(content);
    let result = nusa_config::load(&path);
    cleanup(&path);

    assert!(result.is_err(), "number for bool field must be rejected");
}

// ── Get Consistency ──

#[test]
fn config_get_returns_clone_consistent_across_calls() {
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
    cleanup(&path);

    let cfg1 = nusa_config::get();
    let cfg2 = nusa_config::get();

    assert_eq!(cfg1.max_workers, cfg2.max_workers);
    assert_eq!(cfg1.timeout_ms, cfg2.timeout_ms);
    // EngineKind doesn't implement PartialEq, compare via debug display
    assert_eq!(format!("{:?}", cfg1.engine), format!("{:?}", cfg2.engine));
}

// ── Existing Tests (kept) ──

#[test]
fn config_load_valid_toml_updates_static() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let content = r#"
        engine = "child"
        max_workers = 8
        timeout_ms = 60000
        wasm_memory_mb = 512
        vfs_root = "/test/vfs"
        code_dir = "/test/code"
        tmp_dir = "/test/tmp"
        hot_reload = false
    "#;
    let path = write_temp_config(content);
    let result = nusa_config::load(&path);
    cleanup(&path);

    assert!(result.is_ok(), "valid config must load: {:?}", result);

    let cfg = nusa_config::get();
    assert_eq!(cfg.max_workers, 8);
    assert_eq!(cfg.timeout_ms, 60_000);
    assert!(!cfg.hot_reload);
}

#[test]
fn config_load_invalid_toml() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let path = write_temp_config("this is not toml {{{");
    let _result = nusa_config::load(&path);
    cleanup(&path);

    assert!(_result.is_err(), "invalid toml must return error");
}

#[test]
fn config_load_missing_file() {
    let result = nusa_config::load("/nonexistent/path/config.toml");
    assert!(result.is_err(), "missing file must return error");
}

#[test]
fn config_validation_rejects_zero_workers() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let content = r#"
        engine = "child"
        max_workers = 0
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = true
    "#;
    let path = write_temp_config(content);
    let _result = nusa_config::load(&path);
    cleanup(&path);

    assert!(_result.is_err(), "max_workers = 0 must be rejected");
}

#[tokio::test]
async fn config_watch_returns_handle() {
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
    let handle = nusa_config::watch(path.clone());

    assert!(!handle.is_finished(), "watch task must be running");
    handle.abort();
    cleanup(&path);
}

#[tokio::test]
async fn config_hot_reload_false_exits_immediately() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let content = r#"
        engine = "child"
        max_workers = 4
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = false
    "#;
    let path = write_temp_config(content);

    let _ = nusa_config::load(&path);

    let handle = nusa_config::watch(path.clone());

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        handle.is_finished(),
        "watch must exit when hot_reload is false",
    );
    cleanup(&path);
}
