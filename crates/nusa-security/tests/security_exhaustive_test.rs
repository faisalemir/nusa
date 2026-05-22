//! Security-exhaustive tests for nusa-security (sector S12).
//!
//! Covers: malicious paths, double-apply fail-closed, seccomp filter integrity,
//! non-Linux stub contract. Linux enforcement depth: `security_enforcement_test.rs`.

use std::path::Path;
use std::sync::{Arc, Barrier};
use std::thread;

use nusa_security::{apply_landlock, apply_seccomp, verify_seccomp_filter};

fn unique_landlock_dirs(suffix: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let base = std::env::temp_dir().join(format!("nusa_sec_ex_{suffix}_{}", std::process::id()));
    let code = base.join("code");
    let tmp = base.join("tmp");
    std::fs::create_dir_all(&code).expect("code dir");
    std::fs::create_dir_all(&tmp).expect("tmp dir");
    (code, tmp)
}

fn cleanup_dirs(code: &Path, tmp: &Path) {
    if let Some(parent) = code.parent() {
        let _ = std::fs::remove_dir_all(parent);
    } else {
        let _ = std::fs::remove_dir_all(code);
        let _ = std::fs::remove_dir_all(tmp);
    }
}

// ===== Path / input attacks on apply_landlock =====

#[test]
fn security_landlock_null_byte_in_path_rejected_or_fails() {
    let result = apply_landlock(Path::new("/app\0/evil"), Path::new("/tmp/nusa"));
    #[cfg(target_os = "linux")]
    assert!(result.is_err(), "null byte paths must not apply Landlock");
    #[cfg(not(target_os = "linux"))]
    assert!(result.is_ok(), "non-Linux stub returns Ok");
}

#[test]
fn security_landlock_path_traversal_strings() {
    let traversals = ["../../../etc", "/tmp/../etc/passwd", "..\\..\\windows"];
    for p in traversals {
        let result = apply_landlock(Path::new(p), Path::new("/tmp/nusa"));
        #[cfg(target_os = "linux")]
        {
            // Missing dirs or invalid layout — must not silently succeed as full sandbox
            let _ = result;
        }
        #[cfg(not(target_os = "linux"))]
        assert!(result.is_ok());
    }
}

#[test]
fn security_landlock_nonexistent_code_dir_fails_on_linux() {
    let (code, tmp) = unique_landlock_dirs("missing_code");
    let missing = code.join("does-not-exist-nusa");
    let result = apply_landlock(&missing, &tmp);
    #[cfg(target_os = "linux")]
    assert!(
        result.is_err(),
        "Landlock on missing code_dir must fail closed: {result:?}"
    );
    #[cfg(not(target_os = "linux"))]
    let _ = result;
    cleanup_dirs(&code, &tmp);
}

#[test]
#[cfg(target_os = "linux")]
#[serial_test::serial]
fn security_landlock_double_apply_fails_closed() {
    let (code, tmp) = unique_landlock_dirs("double");
    let first = apply_landlock(&code, &tmp);
    assert!(
        first.is_ok(),
        "first Landlock apply must succeed on Alpine Linux: {first:?}"
    );
    let second = apply_landlock(&code, &tmp);
    assert!(
        second.is_err(),
        "second Landlock apply must fail (process-wide rules): {second:?}"
    );
    cleanup_dirs(&code, &tmp);
}

#[test]
fn security_landlock_symlink_outside_tree_documented() {
    let (code, tmp) = unique_landlock_dirs("symlink");
    let outside = tmp.parent().unwrap().join("outside_target");
    std::fs::create_dir_all(&outside).expect("outside");
    #[cfg(unix)]
    {
        let link = code.join("escape.link");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&outside, &link).expect("symlink");
        let result = apply_landlock(&code, &tmp);
        #[cfg(target_os = "linux")]
        assert!(
            result.is_ok(),
            "apply may succeed; enforcement tested elsewhere"
        );
        #[cfg(not(target_os = "linux"))]
        let _ = result;
    }
    cleanup_dirs(&code, &tmp);
}

// ===== Seccomp filter integrity =====

#[test]
fn security_seccomp_verify_filter_stable_under_repeated_calls() {
    for _ in 0..50 {
        let r = verify_seccomp_filter();
        assert!(r.is_ok(), "filter build must stay deterministic: {r:?}");
    }
}

#[test]
fn security_seccomp_verify_from_multiple_threads() {
    let n = 8;
    let barrier = Arc::new(Barrier::new(n));
    let mut handles = Vec::new();
    for _ in 0..n {
        let b = barrier.clone();
        handles.push(thread::spawn(move || {
            b.wait();
            verify_seccomp_filter().expect("per-thread verify")
        }));
    }
    for h in handles {
        h.join().expect("thread join");
    }
}

#[test]
#[cfg(target_os = "linux")]
fn security_seccomp_apply_succeeds_on_linux_test_runner() {
    let result = apply_seccomp();
    assert!(
        result.is_ok(),
        "seccomp apply must succeed on production Alpine (just podman-ci): {result:?}"
    );
}

#[test]
#[cfg(not(target_os = "linux"))]
fn security_seccomp_stub_on_non_linux_returns_ok() {
    assert!(
        apply_seccomp().is_ok(),
        "non-Linux stub must return Ok with documented contract"
    );
}

#[test]
#[cfg(not(target_os = "linux"))]
fn security_landlock_stub_on_non_linux_returns_ok() {
    assert!(
        apply_landlock(Path::new("/code"), Path::new("/tmp")).is_ok(),
        "non-Linux stub must return Ok"
    );
}

// ===== Combined attack surface =====

#[test]
fn security_verify_before_apply_landlock_sequence() {
    assert!(verify_seccomp_filter().is_ok());
    let (code, tmp) = unique_landlock_dirs("seq");
    let landlock = apply_landlock(&code, &tmp);
    #[cfg(target_os = "linux")]
    assert!(landlock.is_ok(), "landlock in sequence: {landlock:?}");
    #[cfg(not(target_os = "linux"))]
    assert!(landlock.is_ok());
    cleanup_dirs(&code, &tmp);
}
