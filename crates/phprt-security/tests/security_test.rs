//! Integration tests for phprt-security crate — Phase 3 security hardening.
//!
//! Skills applied:
//! - `domain-cloud-native`: Landlock FS rules, Seccomp syscall filter
//! - `m15-anti-pattern`: Security as code, not afterthought
//! - `m06-error-handling`: Stubs return Ok on non-Linux

use phprt_security::landlock::{LandlockRule, apply_landlock as apply_landlock_rules, default_rules};
use phprt_security::seccomp::{SeccompAction, SeccompFilter};
use phprt_security::{apply_landlock, apply_seccomp};
use std::path::Path;

// ---------------------------------------------------------------------------
// LandlockRule tests
// ---------------------------------------------------------------------------

#[test]
fn landlock_rule_read_only_grants_read_only() {
    let rule = LandlockRule::read_only("/app/public");

    assert_eq!(rule.path, std::path::PathBuf::from("/app/public"));
    assert!(rule.read, "read_only must grant read");
    assert!(!rule.write, "read_only must NOT grant write");
    assert!(!rule.execute, "read_only must NOT grant execute");
}

#[test]
fn landlock_rule_read_write_grants_read_and_write() {
    let rule = LandlockRule::read_write("/tmp/phprt");

    assert_eq!(rule.path, std::path::PathBuf::from("/tmp/phprt"));
    assert!(rule.read, "read_write must grant read");
    assert!(rule.write, "read_write must grant write");
    assert!(!rule.execute, "read_write must NOT grant execute");
}

#[test]
fn landlock_rule_read_execute_grants_read_and_execute() {
    let rule = LandlockRule::read_execute("/usr/bin/php");

    assert_eq!(rule.path, std::path::PathBuf::from("/usr/bin/php"));
    assert!(rule.read, "read_execute must grant read");
    assert!(!rule.write, "read_execute must NOT grant write");
    assert!(rule.execute, "read_execute must grant execute");
}

#[test]
fn landlock_rule_accepts_path_buf() {
    let path = std::path::PathBuf::from("/var/www/html");
    let rule = LandlockRule::read_only(&path);
    assert_eq!(rule.path, path);
}

#[test]
fn landlock_rule_debug_and_clone() {
    let rule = LandlockRule::read_only("/etc/app");
    let cloned = rule.clone();

    assert_eq!(format!("{rule:?}"), format!("{cloned:?}"));
    assert_eq!(rule.path, cloned.path);
    assert_eq!(rule.read, cloned.read);
    assert_eq!(rule.write, cloned.write);
}

#[test]
fn landlock_apply_returns_ok() {
    let rules = vec![
        LandlockRule::read_only("/app/public"),
        LandlockRule::read_write("/tmp/phprt"),
    ];
    let result = apply_landlock_rules(&rules);
    assert!(result.is_ok(), "apply_landlock must return Ok");
}

#[test]
fn landlock_apply_with_empty_rules_returns_ok() {
    let result = apply_landlock_rules(&[]);
    assert!(result.is_ok(), "apply_landlock with empty rules must return Ok");
}

#[test]
fn landlock_default_rules_creates_code_and_tmp_rules() {
    let rules = default_rules(Path::new("/app/public"), Path::new("/tmp/phprt"));
    assert_eq!(rules.len(), 2, "Default rules must have 2 entries");

    // First rule: read-only code dir
    assert!(rules[0].read);
    assert!(!rules[0].write);
    assert_eq!(rules[0].path, std::path::PathBuf::from("/app/public"));

    // Second rule: read-write tmp dir
    assert!(rules[1].read);
    assert!(rules[1].write);
    assert_eq!(rules[1].path, std::path::PathBuf::from("/tmp/phprt"));
}

// ---------------------------------------------------------------------------
// Legacy apply_landlock / apply_seccomp tests
// ---------------------------------------------------------------------------

#[test]
fn legacy_apply_landlock_returns_ok() {
    let result = apply_landlock(
        std::path::Path::new("/app/public"),
        std::path::Path::new("/tmp/phprt"),
    );
    assert!(result.is_ok(), "apply_landlock stub must return Ok");
}

#[test]
fn legacy_apply_seccomp_returns_ok() {
    let result = apply_seccomp();
    assert!(result.is_ok(), "apply_seccomp stub must return Ok");
}

// ---------------------------------------------------------------------------
// SeccompFilter tests
// ---------------------------------------------------------------------------

#[test]
fn seccomp_filter_php_runtime_minimal_has_kill_default() {
    let filter = SeccompFilter::php_runtime_minimal();
    assert!(
        matches!(filter.default_action, SeccompAction::Kill),
        "PHP runtime minimal must default to Kill for unlisted syscalls",
    );
}

#[test]
fn seccomp_filter_php_runtime_minimal_contains_expected_syscalls() {
    let filter = SeccompFilter::php_runtime_minimal();
    let syscalls = &filter.allowed_syscalls;

    // Basic I/O
    assert!(syscalls.contains(&0), "must allow read (0)");
    assert!(syscalls.contains(&1), "must allow write (1)");
    assert!(syscalls.contains(&3), "must allow close (3)");
    assert!(syscalls.contains(&2), "must allow open (2)");
    assert!(syscalls.contains(&257), "must allow openat (257)");

    // Memory management
    assert!(syscalls.contains(&9), "must allow mmap (9)");
    assert!(syscalls.contains(&11), "must allow munmap (11)");
    assert!(syscalls.contains(&10), "must allow mprotect (10)");
    assert!(syscalls.contains(&12), "must allow brk (12)");

    // Network operations
    assert!(syscalls.contains(&41), "must allow socket (41)");
    assert!(syscalls.contains(&42), "must allow connect (42)");
    assert!(syscalls.contains(&43), "must allow accept (43)");
    assert!(syscalls.contains(&44), "must allow sendto (44)");
    assert!(syscalls.contains(&45), "must allow recvfrom (45)");

    // Process management
    assert!(syscalls.contains(&56), "must allow clone (56)");
    assert!(syscalls.contains(&39), "must allow getpid (39)");
    assert!(syscalls.contains(&60), "must allow exit (60)");
    assert!(syscalls.contains(&61), "must allow exit_group (61)");
    assert!(syscalls.contains(&62), "must allow wait4 (62)");

    // Misc / architecture-specific
    assert!(syscalls.contains(&158), "must allow arch_prctl (158)");
    assert!(syscalls.contains(&200), "must allow tkill (200)");
    assert!(syscalls.contains(&218), "must allow set_tid_address (218)");
    assert!(syscalls.contains(&228), "must allow clock_gettime (228)");
    assert!(syscalls.contains(&334), "must allow statx (334)");
}

#[test]
fn seccomp_filter_php_runtime_minimal_has_substantial_syscall_count() {
    let filter = SeccompFilter::php_runtime_minimal();
    assert!(
        filter.allowed_syscalls.len() >= 15,
        "PHP runtime minimal must allow at least 15 syscalls, got {}",
        filter.allowed_syscalls.len(),
    );
}

#[test]
fn seccomp_filter_apply_returns_ok() {
    let filter = SeccompFilter::php_runtime_minimal();
    let result = filter.apply();
    assert!(
        result.is_ok(),
        "SeccompFilter::apply must return Ok, got {result:?}",
    );
}

#[test]
fn seccomp_filter_debug_and_clone() {
    let filter = SeccompFilter::php_runtime_minimal();
    let cloned = filter.clone();

    assert_eq!(format!("{filter:?}"), format!("{cloned:?}"));
    assert!(matches!(cloned.default_action, SeccompAction::Kill));
    assert_eq!(cloned.allowed_syscalls, filter.allowed_syscalls);
}

#[test]
fn seccomp_filter_no_duplicate_syscalls() {
    let filter = SeccompFilter::php_runtime_minimal();
    let mut sorted = filter.allowed_syscalls.clone();
    sorted.sort();
    sorted.dedup();

    assert_eq!(
        sorted.len(),
        filter.allowed_syscalls.len(),
        "SeccompFilter should not contain duplicate syscall numbers",
    );
}

#[test]
fn seccomp_action_variants_exist() {
    let _kill = SeccompAction::Kill;
    let _errno = SeccompAction::Errno;
    let _log = SeccompAction::LogAllow;

    // Verify Debug works
    assert_eq!(format!("{:?}", SeccompAction::Kill), "Kill");
    assert_eq!(format!("{:?}", SeccompAction::Errno), "Errno");
    assert_eq!(format!("{:?}", SeccompAction::LogAllow), "LogAllow");
}

// ---------------------------------------------------------------------------
// LandlockAccess enum tests
// ---------------------------------------------------------------------------

#[test]
fn landlock_access_debug_format() {
    // All LandlockRule fields have Debug output
    let rule = LandlockRule::read_only("/test");
    let debug = format!("{rule:?}");
    assert!(debug.contains("/test"), "Debug must include path");
    assert!(debug.contains("true"), "Debug must include boolean flags");
}

#[test]
fn landlock_rule_all_combinations() {
    // Test all rule constructor variants
    let rules = vec![
        LandlockRule::read_only("/app/public"),
        LandlockRule::read_write("/tmp/phprt"),
        LandlockRule::read_execute("/usr/bin/php"),
    ];

    // read_only: read=true, write=false, execute=false
    assert!(rules[0].read && !rules[0].write && !rules[0].execute);

    // read_write: read=true, write=true, execute=false
    assert!(rules[1].read && rules[1].write && !rules[1].execute);

    // read_execute: read=true, write=false, execute=true
    assert!(rules[2].read && !rules[2].write && rules[2].execute);
}
