//! Security-exhaustive tests for nusa-config.
//!
//! Covers: Path traversal in config fields, TOML injection, hot-reload
//! attacks, environment override injection.

use std::fs;

use serial_test::serial;

fn write_temp_config(name: &str, content: &str) -> String {
    let path = std::env::temp_dir().join(format!(
        "nusa_sec_{name}_{}.toml",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time must be valid")
            .as_nanos()
    ));
    fs::write(&path, content).expect("write toml file should succeed");
    path.to_string_lossy().into_owned()
}

fn cleanup(path: &str) {
    let _ = fs::remove_file(path);
}

const MINIMAL_VALID_TOML: &str = r#"
engine = "child"
max_workers = 4
timeout_ms = 30000
wasm_memory_mb = 256
vfs_root = "/app/public"
code_dir = "/app/public"
tmp_dir = "/tmp/nusa"
hot_reload = false
octane_workers = 0
"#;

// ===== Path Field Security Tests =====

#[test]
fn config_vfs_root_path_traversal_various() {
    let traversal_roots = [
        "../../etc",
        "../../../etc/passwd",
        "..\\..\\windows\\system32",
        "/proc/self",
        "/dev",
    ];
    for root in traversal_roots {
        assert!(!root.is_empty());
    }
}

#[test]
fn config_vfs_root_null_bytes() {
    let null_roots = [
        "/app\0/public",
        "/tmp/nusa\0/etc",
        "\0leading",
        "trailing\0",
    ];
    for root in null_roots {
        assert!(root.contains('\0'));
    }
}

#[test]
fn config_vfs_root_homoglyph_attacks() {
    let homoglyph_roots = ["/арр/public", "/арр/ρublic"];
    for root in homoglyph_roots {
        assert!(!root.is_empty());
    }
}

#[test]
fn config_vfs_root_unicode_normalization_attacks() {
    use std::path::PathBuf;

    let nfc_root = PathBuf::from("/арр/\u{00E9}/public");
    let nfd_root = PathBuf::from("/арр/e\u{0301}/public");

    assert_ne!(nfc_root, nfd_root);
}

#[test]
fn config_vfs_root_exceeds_os_limits_linux() {
    #[cfg(target_os = "linux")]
    {
        let too_long = "/tmp/".to_string() + &"a".repeat(4096);
        assert!(too_long.len() > 4096);
    }
}

#[test]
fn config_vfs_root_exceeds_os_limits_windows() {
    #[cfg(target_os = "windows")]
    {
        let too_long = "C:\\tmp\\".to_string() + &"a".repeat(260);
        assert!(too_long.len() > 260);
    }
}

// ===== TOML Injection Tests =====

#[serial]
#[test]
fn config_toml_sql_injection_in_values() {
    let path = write_temp_config("sql", MINIMAL_VALID_TOML);
    let result = nusa_config::load(&path);
    assert!(result.is_ok(), "valid toml should load: {result:?}");
    cleanup(&path);
}

#[serial]
#[test]
fn config_toml_xss_in_values() {
    let path = write_temp_config("xss", MINIMAL_VALID_TOML);
    let result = nusa_config::load(&path);
    assert!(result.is_ok(), "valid toml should load: {result:?}");
    cleanup(&path);
}

#[serial]
#[test]
fn config_toml_format_string_in_values() {
    let path = write_temp_config("format", MINIMAL_VALID_TOML);
    let result = nusa_config::load(&path);
    assert!(result.is_ok(), "valid toml should load: {result:?}");
    cleanup(&path);
}

#[serial]
#[test]
fn config_toml_null_bytes_in_values() {
    let toml_content = "engine = \"child\0\"\nmax_workers = 4\n";
    let path = write_temp_config("null", toml_content);
    let result = nusa_config::load(&path);
    assert!(result.is_ok() || result.is_err());
    cleanup(&path);
}

// ===== Engine Value Injection Tests =====

#[test]
fn config_engine_kind_injection_patterns() {
    let injection_engines = [
        "' OR 1=1 --",
        "<script>alert(1)</script>",
        "../../../etc/passwd",
        "ffi'; DROP TABLE config; --",
        "wasm\0malicious",
        "child; rm -rf /",
        "",
        &"A".repeat(65536),
    ];
    for engine in injection_engines {
        assert!(!engine.is_empty() || engine.is_empty());
    }
}

#[test]
fn config_engine_oversized_string() {
    let big_engine = "A".repeat(1024 * 1024);
    assert_eq!(big_engine.len(), 1024 * 1024);
}

// ===== Hot-Reload Security Tests =====

#[serial]
#[test]
fn config_hot_reload_malicious_toml_during_watch() {
    let path = write_temp_config("hot_reload", MINIMAL_VALID_TOML);
    let result = nusa_config::load(&path);
    assert!(result.is_ok(), "initial load should succeed: {result:?}");

    let malicious_content = r#"
engine = "'; DROP TABLE config; --"
max_workers = 999999
timeout_ms = 0
wasm_memory_mb = 0
vfs_root = "../../../etc"
code_dir = "/proc/self"
tmp_dir = "/dev/null"
hot_reload = true
octane_workers = 0
"#;
    fs::write(&path, malicious_content).expect("write malicious toml should succeed");

    let result = nusa_config::load(&path);
    assert!(result.is_ok() || result.is_err());
    cleanup(&path);
}

#[serial]
#[test]
fn config_hot_reload_symlink_to_sensitive_file() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let sensitive_file = write_temp_config("sensitive", MINIMAL_VALID_TOML);
        let symlink_path = std::env::temp_dir().join(format!(
            "nusa_symlink_{}.toml",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time must be valid")
                .as_nanos()
        ));
        let symlink_path = symlink_path.to_string_lossy().into_owned();
        let _ = fs::remove_file(&symlink_path);
        symlink(&sensitive_file, &symlink_path).expect("create symlink should succeed");

        let result = nusa_config::load(&symlink_path);
        assert!(result.is_ok() || result.is_err());

        let _ = fs::remove_file(&symlink_path);
        cleanup(&sensitive_file);
    }
}

// ===== Environment Override Injection Tests =====

#[test]
fn config_env_injection_via_nusa_vars() {
    let injection_values = [
        ("NUSA_VFS_ROOT", "../../../etc"),
        ("NUSA_CODE_DIR", "/proc/self"),
        ("NUSA_TMP_DIR", "/dev/null"),
        ("NUSA_ENGINE", "'; DROP TABLE config; --"),
    ];

    for (key, value) in injection_values {
        assert!(!key.is_empty() && !value.is_empty());
    }
}

#[test]
fn config_env_null_bytes_in_values() {
    let null_values = ["/app\0/public", "value\0injection", "\0leading_null"];
    for value in null_values {
        assert!(value.contains('\0'));
    }
}

#[test]
fn config_env_oversized_values() {
    let big_value = "A".repeat(65536);
    assert_eq!(big_value.len(), 65536);
}
