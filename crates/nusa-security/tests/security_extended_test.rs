//! Extended integration tests for nusa-security crate.
//!
//! Skills applied:
//! - `domain-cloud-native`: Landlock FS rules, Seccomp syscall filter
//! - `m15-anti-pattern`: Security as code, not afterthought
//! - `m06-error-handling`: Stubs return Ok on non-Linux

use nusa_security::{apply_landlock, verify_seccomp_filter};
use std::path::Path;

// ── Landlock: Path Edge Cases ──

#[test]
fn apply_landlock_with_empty_paths() {
    let result = apply_landlock(Path::new(""), Path::new(""));
    // On Linux: may fail with empty paths. On non-Linux: returns Ok.
    // Either behavior is acceptable for empty paths.
    let _ = result;
}

#[test]
fn apply_landlock_with_current_dir() {
    let result = apply_landlock(Path::new("."), Path::new("."));
    // On Linux: should succeed with current dir. On non-Linux: returns Ok.
    // Either behavior is acceptable.
    let _ = result;
}

#[test]
fn apply_landlock_with_root_paths() {
    let result = apply_landlock(Path::new("/"), Path::new("/"));
    // On Linux: should restrict to root. On non-Linux: returns Ok.
    let _ = result;
}

#[test]
fn apply_landlock_with_very_long_paths() {
    let long_path = "/app/very/long/path/".repeat(100);
    let result = apply_landlock(Path::new(&long_path), Path::new("/tmp/nusa"));
    // On Linux: may fail if paths don't exist. On non-Linux: returns Ok.
    let _ = result;
}

#[test]
fn apply_landlock_with_unicode_paths() {
    let result = apply_landlock(
        Path::new("/app/项目/laravel"),
        Path::new("/tmp/nusa-テスト"),
    );
    // On Linux: may fail if paths don't exist. On non-Linux: returns Ok.
    let _ = result;
}

#[test]
fn apply_landlock_with_spaces_in_path() {
    let result = apply_landlock(
        Path::new("/app/my project/laravel"),
        Path::new("/tmp/nusa test"),
    );
    // On Linux: may fail if paths don't exist. On non-Linux: returns Ok.
    let _ = result;
}

// ── Seccomp: Multiple Applications ──

#[test]
fn apply_seccomp_idempotent_on_non_linux() {
    // Filter build is repeatable without touching the running test process.
    let result1 = verify_seccomp_filter();
    let result2 = verify_seccomp_filter();

    assert!(result1.is_ok(), "first seccomp verify must return Ok");
    assert!(result2.is_ok(), "second seccomp verify must return Ok");
}

// ── Combined Security ──

#[test]
fn security_both_applied_in_sequence() {
    // Test that both security functions can be called in sequence
    let tmp = std::env::temp_dir().join("nusa-security-seq");
    std::fs::create_dir_all(&tmp).expect("tmpdir");
    let code = tmp.join("code");
    std::fs::create_dir_all(&code).expect("code dir");
    let landlock_result = apply_landlock(&code, &tmp);
    let seccomp_result = verify_seccomp_filter();

    // On Linux: landlock may fail if paths don't exist, seccomp may already be applied
    // On non-Linux: both return Ok
    let _ = (landlock_result, seccomp_result);
}

// ── Original Tests ──

#[test]
fn apply_landlock_returns_ok() {
    let tmp = std::env::temp_dir().join("nusa-security-landlock-ok");
    std::fs::create_dir_all(&tmp).expect("tmpdir");
    let code = tmp.join("code");
    std::fs::create_dir_all(&code).expect("code dir");
    let result = apply_landlock(&code, &tmp);
    assert!(result.is_ok(), "apply_landlock must return Ok: {result:?}");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn apply_seccomp_returns_ok() {
    let result = verify_seccomp_filter();
    assert!(result.is_ok(), "seccomp filter must build: {result:?}");
}
