//! Platform-specific tests for configuration.
//!
//! rust-test §Platform-Specific Tests
//! Path separators, line endings, case sensitivity, Windows UNC paths.

use std::path::{Path, PathBuf};

// ── Path Separators ──

/// Path separators: `/` vs `\` handled correctly on current platform.
#[test]
fn platform_path_separator_forward_slash() {
    let path = Path::new("logs/app.log");
    assert!(path.is_relative());
    assert_eq!(path.to_str().unwrap(), "logs/app.log");
}

#[test]
#[cfg(windows)]
fn platform_path_separator_backslash_windows() {
    let path = Path::new(r"logs\app.log");
    assert!(path.is_relative());
    // On Windows, backslash is native separator
    assert_eq!(path.components().count(), 2);
}

#[test]
#[cfg(unix)]
fn platform_path_separator_backslash_unix() {
    let path = Path::new(r"logs\app.log");
    // On Unix, backslash is just a regular character in filename
    assert!(path.is_relative());
    // Single component because `\` is not a separator on Unix
    assert_eq!(path.components().count(), 1);
}

// ── Line Endings ──

/// Config file line endings: `\n` vs `\r\n` handled correctly.
#[test]
fn platform_lineending_unix_style() {
    let content = "key1=value1\nkey2=value2\n";
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], "key1=value1");
    assert_eq!(lines[1], "key2=value2");
}

#[test]
fn platform_lineending_windows_style() {
    // Rust's lines() handles \r\n correctly
    let content = "key1=value1\r\nkey2=value2\r\n";
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], "key1=value1");
    assert_eq!(lines[1], "key2=value2");
}

#[test]
fn platform_lineending_mixed() {
    // Mixed line endings should still parse correctly
    let content = "key1=value1\r\nkey2=value2\nkey3=value3\r\n";
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 3);
}

// ── Case Sensitivity ──

/// Case sensitivity: file name comparisons on current platform.
#[test]
fn platform_case_sensitivity_path_comparison() {
    // Rust's Path comparison is case-sensitive on all platforms
    let path_a = Path::new("Config.toml");
    let path_b = Path::new("config.toml");

    // These are different paths regardless of filesystem case sensitivity
    assert_ne!(path_a, path_b);
}

#[test]
fn platform_case_sensitivity_extension() {
    let path_a = Path::new("file.TXT");
    let path_b = Path::new("file.txt");

    assert_ne!(path_a, path_b);
    assert_ne!(path_a.extension(), path_b.extension());
}

// ── Windows UNC Paths ──

#[test]
#[cfg(windows)]
fn platform_unc_path_parsing() {
    // UNC paths like \\server\share\file
    let unc = Path::new(r"\\server\share\file.txt");
    assert!(unc.is_absolute());
    // UNC paths have 4+ components: server, share, file
    assert!(unc.components().count() >= 3);
}

#[test]
#[cfg(windows)]
fn platform_unc_path_prefix() {
    let unc = Path::new(r"\\?\C:\path\to\file.txt");
    assert!(unc.is_absolute());
}

// ── Platform-Specific Path Features ──

#[test]
#[cfg(unix)]
fn platform_unix_absolute_path() {
    let path = Path::new("/etc/passwd");
    assert!(path.is_absolute());
    assert!(path.starts_with("/"));
}

#[test]
#[cfg(windows)]
fn platform_windows_absolute_path_drive() {
    let path = Path::new("C:\\Windows\\System32");
    assert!(path.is_absolute());
    assert!(path.has_root());
}

#[test]
fn platform_relative_path_current_dir() {
    let path = Path::new("./config.toml");
    assert!(path.is_relative());
}

#[test]
fn platform_relative_path_parent_dir() {
    let path = Path::new("../config.toml");
    assert!(path.is_relative());
}

// ── PathBuf Construction ──

#[test]
fn platform_pathbuf_join_uses_native_separator() {
    let base = PathBuf::from("logs");
    let joined = base.join("app.log");

    #[cfg(windows)]
    assert!(joined.to_string_lossy().contains('\\'));

    #[cfg(unix)]
    assert!(joined.to_string_lossy().contains('/'));
}

// ── Config Path Resolution ──

#[test]
fn platform_config_file_extension_toml() {
    let path = Path::new("config.toml");
    assert_eq!(path.extension().unwrap(), "toml");
}

#[test]
fn platform_config_file_extension_toml_case_variants() {
    // Config files might have different case extensions
    assert_eq!(Path::new("config.TOML").extension().unwrap(), "TOML");
    assert_eq!(Path::new("config.Toml").extension().unwrap(), "Toml");
}
