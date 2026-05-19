//! Integration tests for phprt-config crate
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
        "phprt_cfg_test_{}.toml",
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
    let result = phprt_config::load(&path);
    cleanup(&path);

    assert!(result.is_ok(), "valid config must load: {:?}", result);

    // Verify the static was updated
    let cfg = phprt_config::get();
    assert_eq!(cfg.max_workers, 8);
    assert_eq!(cfg.timeout_ms, 60_000);
    assert!(!cfg.hot_reload);
}

#[test]
fn config_load_invalid_toml() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let path = write_temp_config("this is not toml {{{");
    let _result = phprt_config::load(&path);
    cleanup(&path);

    assert!(_result.is_err(), "invalid toml must return error");
}

#[test]
fn config_load_missing_file() {
    // This test doesn't mutate the static, so no lock needed
    let result = phprt_config::load("/nonexistent/path/config.toml");
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
    let _result = phprt_config::load(&path);
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
    let handle = phprt_config::watch(path.clone());

    assert!(!handle.is_finished(), "watch task must be running");
    handle.abort();
    cleanup(&path);
}

#[tokio::test]
async fn config_hot_reload_false_exits_immediately() {
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    // Write config with hot_reload = false
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
    
    // Load the config first so get() returns hot_reload = false
    let _ = phprt_config::load(&path);
    
    let handle = phprt_config::watch(path.clone());

    // With hot_reload = false, the task should exit immediately
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        handle.is_finished(),
        "watch must exit when hot_reload is false",
    );
    cleanup(&path);
}
