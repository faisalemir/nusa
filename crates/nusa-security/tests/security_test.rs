//! Integration tests for nusa-security crate — Phase 3 security hardening.
//!
//! Skills applied:
//! - `domain-cloud-native`: Landlock FS rules, Seccomp syscall filter
//! - `m15-anti-pattern`: Security as code, not afterthought
//! - `m06-error-handling`: Stubs return Ok on non-Linux

use nusa_security::{apply_landlock, verify_seccomp_filter};

#[test]
fn apply_landlock_returns_ok() {
    let tmp = std::env::temp_dir().join("nusa-security-test-landlock");
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
