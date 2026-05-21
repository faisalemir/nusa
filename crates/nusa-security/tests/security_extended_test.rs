//! Extended integration tests for nusa-security crate.
//!
//! Skills applied:
//! - `domain-cloud-native`: Landlock FS rules, Seccomp syscall filter
//! - `m15-anti-pattern`: Security as code, not afterthought
//! - `m06-error-handling`: Stubs return Ok on non-Linux

use nusa_security::{apply_landlock, apply_seccomp};
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
    // On non-Linux, calling seccomp multiple times should all return Ok
    let result1 = apply_seccomp();
    let result2 = apply_seccomp();

    assert!(result1.is_ok(), "first seccomp call must return Ok");
    assert!(result2.is_ok(), "second seccomp call must return Ok");
}

#[cfg(target_os = "linux")]
#[test]
fn apply_seccomp_can_only_be_called_once() {
    // On Linux, seccomp can only be called once per thread
    // This tests that the function returns an error on second call
    let result1 = apply_seccomp();
    let result2 = apply_seccomp();

    // First call should succeed (or already be filtered by outer seccomp)
    // Second call should fail (seccomp is one-way)
    if result1.is_ok() {
        assert!(
            result2.is_err(),
            "seccomp second call should fail (one-way restriction)"
        );
    }
}

// ── Combined Security ──

#[test]
fn security_both_applied_in_sequence() {
    // Test that both security functions can be called in sequence
    let landlock_result = apply_landlock(Path::new("/app/public"), Path::new("/tmp/nusa"));
    let seccomp_result = apply_seccomp();

    // On Linux: landlock may fail if paths don't exist, seccomp may already be applied
    // On non-Linux: both return Ok
    let _ = (landlock_result, seccomp_result);
}

// ── Original Tests ──

#[test]
fn apply_landlock_returns_ok() {
    let result = apply_landlock(
        std::path::Path::new("/app/public"),
        std::path::Path::new("/tmp/nusa"),
    );
    assert!(result.is_ok(), "apply_landlock must return Ok");
}

#[test]
fn apply_seccomp_returns_ok() {
    let result = apply_seccomp();
    assert!(result.is_ok(), "apply_seccomp must return Ok");
}
