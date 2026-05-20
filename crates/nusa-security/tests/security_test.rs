//! Integration tests for nusa-security crate — Phase 3 security hardening.
//!
//! Skills applied:
//! - `domain-cloud-native`: Landlock FS rules, Seccomp syscall filter
//! - `m15-anti-pattern`: Security as code, not afterthought
//! - `m06-error-handling`: Stubs return Ok on non-Linux

use nusa_security::{apply_landlock, apply_seccomp};

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
