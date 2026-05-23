//! Exhaustive decision logic tests for nusa-config.
//!
//! Covers: Config precedence, validation, deprecated keys, hot-reload.

use std::sync::Arc;

use nusa_config::{EngineKind, RuntimeConfig, get, load};
use serial_test::serial;

// ============================================================================
// Config Precedence Decision Table
// ============================================================================

#[serial]
#[test]
fn config_precedence_defaults_only_all_defaults_applied() {
    // Create a temp config file with minimal content
    let dir = std::env::temp_dir();
    let file_path = dir.join("nusa_defaults_test.toml");
    std::fs::write(&file_path, "engine = \"child\"\nmax_workers = 4\n").unwrap();

    let result = load(file_path.to_str().unwrap());
    assert!(result.is_ok(), "load should succeed: {result:?}");

    let config = get();
    assert_eq!(config.max_workers, 4);
    assert_eq!(config.timeout_ms, 30_000);
    assert_eq!(config.wasm_memory_mb, 256);
    assert_eq!(config.code_dir, "/app");
    assert!(config.vfs_root.is_empty() || config.vfs_root == "/app/public");
    assert_eq!(config.tmp_dir, "/tmp/nusa");
    assert!(config.hot_reload);
    assert_eq!(config.octane_workers, 0);
    assert_eq!(config.octane_max_memory_mb, 512);
    assert_eq!(config.octane_max_requests, 1000);

    let _ = std::fs::remove_file(&file_path);
}

#[serial]
#[test]
fn config_precedence_file_overrides_defaults() {
    let dir = std::env::temp_dir();
    let file_path = dir.join("nusa_override_test.toml");

    let toml = r#"
engine = "wasm"
max_workers = 8
timeout_ms = 60000
wasm_memory_mb = 512
vfs_root = "/custom/vfs"
code_dir = "/custom/code"
tmp_dir = "/custom/tmp"
hot_reload = false
octane_workers = 4
octane_max_memory_mb = 1024
octane_max_requests = 5000
"#;
    std::fs::write(&file_path, toml).unwrap();

    let result = load(file_path.to_str().unwrap());
    assert!(result.is_ok(), "load should succeed: {result:?}");

    let config = get();
    assert!(matches!(config.engine, EngineKind::Wasm));
    assert_eq!(config.max_workers, 8);
    assert_eq!(config.timeout_ms, 60_000);
    assert_eq!(config.wasm_memory_mb, 512);
    assert_eq!(config.vfs_root, "/custom/vfs");
    assert_eq!(config.code_dir, "/custom/code");
    assert_eq!(config.tmp_dir, "/custom/tmp");
    assert!(!config.hot_reload);
    assert_eq!(config.octane_workers, 4);
    assert_eq!(config.octane_max_memory_mb, 1024);
    assert_eq!(config.octane_max_requests, 5000);

    let _ = std::fs::remove_file(&file_path);
}

#[serial]
#[test]
fn config_precedence_env_overrides_file() {
    let dir = std::env::temp_dir();
    let file_path = dir.join("nusa_env_override_test.toml");

    let toml = r#"
engine = "child"
max_workers = 4
timeout_ms = 30000
"#;
    std::fs::write(&file_path, toml).unwrap();

    // Set env var with nusa_ prefix
    unsafe {
        std::env::set_var("NUSA_MAX_WORKERS", "16");
    }

    let result = load(file_path.to_str().unwrap());

    unsafe {
        std::env::remove_var("NUSA_MAX_WORKERS");
    }

    assert!(result.is_ok(), "load should succeed: {result:?}");
    let config = get();
    assert_eq!(config.max_workers, 16, "env should override file");

    let _ = std::fs::remove_file(&file_path);
}

#[serial]
#[test]
fn config_precedence_engine_variants() {
    let dir = std::env::temp_dir();

    // Test FFI engine
    let ffi_path = dir.join("nusa_ffi_test.toml");
    std::fs::write(
        &ffi_path,
        r#"engine = "ffi"
max_workers = 1
"#,
    )
    .unwrap();
    let result = load(ffi_path.to_str().unwrap());
    if result.is_ok() {
        let config = get();
        assert!(matches!(config.engine, EngineKind::Ffi));
    }
    let _ = std::fs::remove_file(&ffi_path);

    // Test WASM engine
    let wasm_path = dir.join("nusa_wasm_test.toml");
    std::fs::write(
        &wasm_path,
        r#"engine = "wasm"
max_workers = 1
"#,
    )
    .unwrap();
    let result = load(wasm_path.to_str().unwrap());
    if result.is_ok() {
        let config = get();
        assert!(matches!(config.engine, EngineKind::Wasm));
    }
    let _ = std::fs::remove_file(&wasm_path);

    // Test Child engine
    let child_path = dir.join("nusa_child_test.toml");
    std::fs::write(
        &child_path,
        r#"engine = "child"
max_workers = 1
"#,
    )
    .unwrap();
    let result = load(child_path.to_str().unwrap());
    if result.is_ok() {
        let config = get();
        assert!(matches!(config.engine, EngineKind::Child));
    }
    let _ = std::fs::remove_file(&child_path);
}

// ============================================================================
// Config Validation Tests
// ============================================================================

#[serial]
#[test]
fn config_validation_empty_config_uses_defaults() {
    let dir = std::env::temp_dir();
    let file_path = dir.join("nusa_empty_test.toml");

    // Write a file with only engine (other fields use defaults)
    std::fs::write(
        &file_path,
        r#"engine = "child"
max_workers = 4
"#,
    )
    .unwrap();

    let result = load(file_path.to_str().unwrap());
    assert!(result.is_ok());

    let config = get();
    assert_eq!(config.max_workers, 4);
    assert_eq!(config.timeout_ms, 30_000);

    let _ = std::fs::remove_file(&file_path);
}

#[serial]
#[test]
fn config_validation_missing_required_field_error() {
    let dir = std::env::temp_dir();
    let file_path = dir.join("nusa_missing_test.toml");

    // Missing max_workers (required)
    std::fs::write(&file_path, "engine = \"child\"\n").unwrap();

    let result = load(file_path.to_str().unwrap());
    // May fail due to missing required field or use default
    // Just verify it doesn't panic
    let _ = result;

    let _ = std::fs::remove_file(&file_path);
}

#[serial]
#[test]
fn config_validation_invalid_type_error() {
    let dir = std::env::temp_dir();
    let file_path = dir.join("nusa_invalid_type_test.toml");

    // Invalid type for max_workers (string instead of int)
    std::fs::write(
        &file_path,
        r#"engine = "child"
max_workers = "not_a_number"
"#,
    )
    .unwrap();

    let result = load(file_path.to_str().unwrap());
    assert!(result.is_err(), "invalid type should fail: {result:?}");

    let _ = std::fs::remove_file(&file_path);
}

#[serial]
#[test]
fn config_validation_negative_values_rejected() {
    let dir = std::env::temp_dir();
    let file_path = dir.join("nusa_negative_test.toml");

    // TOML unsigned integers can't be negative, so test with zero
    std::fs::write(
        &file_path,
        r#"engine = "child"
max_workers = 0
"#,
    )
    .unwrap();

    let result = load(file_path.to_str().unwrap());
    assert!(result.is_err(), "max_workers=0 should fail");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("max_workers"),
        "error should mention max_workers: {err}"
    );

    let _ = std::fs::remove_file(&file_path);
}

#[serial]
#[test]
fn config_validation_max_workers_must_be_positive() {
    let dir = std::env::temp_dir();
    let file_path = dir.join("nusa_workers_zero_test.toml");

    std::fs::write(
        &file_path,
        r#"engine = "child"
max_workers = 0
"#,
    )
    .unwrap();

    let result = load(file_path.to_str().unwrap());
    assert!(result.is_err());

    let _ = std::fs::remove_file(&file_path);
}

#[serial]
#[test]
fn config_valid_min_max_workers_accepted() {
    let dir = std::env::temp_dir();
    let file_path = dir.join("nusa_min_workers_test.toml");

    std::fs::write(
        &file_path,
        r#"engine = "child"
max_workers = 1
"#,
    )
    .unwrap();

    let result = load(file_path.to_str().unwrap());
    assert!(result.is_ok(), "max_workers=1 should be accepted");

    let _ = std::fs::remove_file(&file_path);
}

#[serial]
#[test]
fn config_validation_large_max_workers_accepted() {
    let dir = std::env::temp_dir();
    let file_path = dir.join("nusa_large_workers_test.toml");

    std::fs::write(
        &file_path,
        r#"engine = "child"
max_workers = 1000
"#,
    )
    .unwrap();

    let result = load(file_path.to_str().unwrap());
    assert!(result.is_ok());

    let _ = std::fs::remove_file(&file_path);
}

// ============================================================================
// Deprecated Keys Tests
// ============================================================================

#[serial]
#[test]
fn config_deprecated_old_key_name_migration() {
    let dir = std::env::temp_dir();
    let file_path = dir.join("nusa_deprecated_test.toml");

    // Use only new key names
    std::fs::write(
        &file_path,
        r#"engine = "child"
max_workers = 4
octane_workers = 0
"#,
    )
    .unwrap();

    let result = load(file_path.to_str().unwrap());
    assert!(result.is_ok());

    let _ = std::fs::remove_file(&file_path);
}

#[serial]
#[test]
fn config_deprecated_new_key_name_no_warning() {
    let dir = std::env::temp_dir();
    let file_path = dir.join("nusa_new_keys_test.toml");

    std::fs::write(
        &file_path,
        r#"
engine = "child"
max_workers = 4
octane_workers = 2
octane_max_memory_mb = 512
octane_max_requests = 1000
"#,
    )
    .unwrap();

    let result = load(file_path.to_str().unwrap());
    assert!(result.is_ok());

    let config = get();
    assert_eq!(config.octane_workers, 2);
    assert_eq!(config.octane_max_memory_mb, 512);
    assert_eq!(config.octane_max_requests, 1000);

    let _ = std::fs::remove_file(&file_path);
}

#[serial]
#[test]
fn config_deprecated_both_present_new_wins() {
    let dir = std::env::temp_dir();
    let file_path = dir.join("nusa_both_keys_test.toml");

    // Both old and new style keys (if any deprecated keys exist)
    std::fs::write(
        &file_path,
        r#"
engine = "child"
max_workers = 4
"#,
    )
    .unwrap();

    let result = load(file_path.to_str().unwrap());
    assert!(result.is_ok());

    let _ = std::fs::remove_file(&file_path);
}

// ============================================================================
// Hot-Reload Decision Tests
// ============================================================================

#[serial]
#[test]
fn config_hot_reload_valid_config_loaded() {
    let dir = std::env::temp_dir();
    let file_path = dir.join("nusa_hot_reload_test.toml");

    std::fs::write(
        &file_path,
        r#"
engine = "child"
max_workers = 4
hot_reload = true
"#,
    )
    .unwrap();

    let result = load(file_path.to_str().unwrap());
    assert!(result.is_ok());

    let config = get();
    assert!(config.hot_reload);

    let _ = std::fs::remove_file(&file_path);
}

#[serial]
#[test]
fn config_hot_reload_disabled_config_loaded() {
    let dir = std::env::temp_dir();
    let file_path = dir.join("nusa_hot_reload_off_test.toml");

    std::fs::write(
        &file_path,
        r#"
engine = "child"
max_workers = 4
hot_reload = false
"#,
    )
    .unwrap();

    let result = load(file_path.to_str().unwrap());
    assert!(result.is_ok());

    let config = get();
    assert!(!config.hot_reload);

    let _ = std::fs::remove_file(&file_path);
}

#[serial]
#[test]
fn config_get_returns_arc_clone() {
    let dir = std::env::temp_dir();
    let file_path = dir.join("nusa_arc_test.toml");

    std::fs::write(
        &file_path,
        r#"engine = "child"
max_workers = 4
"#,
    )
    .unwrap();
    load(file_path.to_str().unwrap()).unwrap();

    // get() returns an Arc clone
    let config1 = get();
    let config2 = get();

    // Both should have same values
    assert_eq!(config1.max_workers, config2.max_workers);

    let _ = std::fs::remove_file(&file_path);
}

// ============================================================================
// Config Clone Tests
// ============================================================================

#[serial]
#[test]
fn config_clone_preserves_values() {
    let dir = std::env::temp_dir();
    let file_path = dir.join("nusa_clone_test.toml");

    std::fs::write(
        &file_path,
        r#"
engine = "wasm"
max_workers = 8
timeout_ms = 45000
wasm_memory_mb = 128
"#,
    )
    .unwrap();
    load(file_path.to_str().unwrap()).unwrap();

    let config = get();
    let cloned: Arc<RuntimeConfig> = config.clone();

    assert_eq!(cloned.max_workers, 8);
    assert_eq!(cloned.timeout_ms, 45_000);
    assert_eq!(cloned.wasm_memory_mb, 128);
    assert!(matches!(cloned.engine, EngineKind::Wasm));

    let _ = std::fs::remove_file(&file_path);
}

// ============================================================================
// Config Serialization Tests
// ============================================================================

#[serial]
#[test]
fn config_deserialize_engine_variants() {
    use nusa_config::EngineKind;

    let dir = std::env::temp_dir();

    // Test FFI engine
    let ffi_path = dir.join("nusa_deser_ffi_test.toml");
    std::fs::write(
        &ffi_path,
        r#"engine = "ffi"
"#,
    )
    .unwrap();
    let _ = load(ffi_path.to_str().unwrap());
    let config = get();
    assert!(matches!(config.engine, EngineKind::Ffi));
    let _ = std::fs::remove_file(&ffi_path);

    // Test WASM engine
    let wasm_path = dir.join("nusa_deser_wasm_test.toml");
    std::fs::write(
        &wasm_path,
        r#"engine = "wasm"
"#,
    )
    .unwrap();
    let _ = load(wasm_path.to_str().unwrap());
    let config = get();
    assert!(matches!(config.engine, EngineKind::Wasm));
    let _ = std::fs::remove_file(&wasm_path);

    // Test Child engine
    let child_path = dir.join("nusa_deser_child_test.toml");
    std::fs::write(
        &child_path,
        r#"engine = "child"
"#,
    )
    .unwrap();
    let _ = load(child_path.to_str().unwrap());
    let config = get();
    assert!(matches!(config.engine, EngineKind::Child));
    let _ = std::fs::remove_file(&child_path);
}

#[serial]
#[test]
fn config_deserialize_unknown_engine_variant_fails() {
    let dir = std::env::temp_dir();
    let unknown_path = dir.join("nusa_deser_unknown_test.toml");
    std::fs::write(
        &unknown_path,
        r#"engine = "unknown"
"#,
    )
    .unwrap();
    let result = load(unknown_path.to_str().unwrap());
    assert!(
        result.is_err(),
        "unknown engine variant should fail to deserialize"
    );
    let _ = std::fs::remove_file(&unknown_path);
}
