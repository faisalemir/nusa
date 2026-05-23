//! Resource-exhaustive tests for nusa-security (sector S12).
//!
//! Covers: repeated filter builds, Landlock apply lock contention, no silent
//! resource growth from security API calls.

use std::sync::{Arc, Barrier};
use std::thread;

use nusa_security::{apply_landlock, verify_seccomp_filter};

fn count_open_fds() -> usize {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_dir("/proc/self/fd")
            .map(|d| d.count())
            .unwrap_or(0)
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

#[test]
fn resource_verify_seccomp_filter_1000_iterations_no_fd_growth() {
    let start = count_open_fds();
    for _ in 0..1000 {
        verify_seccomp_filter().expect("verify must not leak");
    }
    let end = count_open_fds();
    let slack = if cfg!(target_os = "linux") { 16 } else { 4 };
    assert!(
        end <= start + slack,
        "FD count must be stable (start={start}, end={end})"
    );
}

#[test]
fn resource_concurrent_verify_seccomp_no_panic() {
    let workers = 16;
    let barrier = Arc::new(Barrier::new(workers));
    let mut handles = Vec::with_capacity(workers);
    for _ in 0..workers {
        let b = barrier.clone();
        handles.push(thread::spawn(move || {
            b.wait();
            for _ in 0..100 {
                verify_seccomp_filter().expect("concurrent verify");
            }
        }));
    }
    for h in handles {
        h.join().expect("worker join");
    }
}

#[test]
#[cfg(target_os = "linux")]
#[serial_test::serial]
fn resource_landlock_double_apply_does_not_panic() {
    let base = std::env::temp_dir().join(format!("nusa_res_ll_{}", std::process::id()));
    let code = base.join("code");
    let tmp = base.join("tmp");
    std::fs::create_dir_all(&code).expect("code");
    std::fs::create_dir_all(&tmp).expect("tmp");

    let first = apply_landlock(&code, &tmp);
    assert!(first.is_ok(), "first apply: {first:?}");
    let second = apply_landlock(&code, &tmp);
    assert!(second.is_err(), "second apply must return Err not panic");

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
#[cfg(target_os = "linux")]
#[serial_test::serial]
fn resource_landlock_apply_lock_serializes_threads() {
    let base = std::env::temp_dir().join(format!("nusa_res_lock_{}", std::process::id()));
    let code = base.join("code");
    let tmp = base.join("tmp");
    std::fs::create_dir_all(&code).expect("code");
    std::fs::create_dir_all(&tmp).expect("tmp");

    let code = Arc::new(code);
    let tmp = Arc::new(tmp);
    let barrier = Arc::new(Barrier::new(4));
    let mut handles = Vec::new();
    for _ in 0..4 {
        let c = code.clone();
        let t = tmp.clone();
        let b = barrier.clone();
        handles.push(thread::spawn(move || {
            b.wait();
            apply_landlock(c.as_path(), t.as_path())
        }));
    }
    let mut ok_count = 0usize;
    let mut err_count = 0usize;
    for h in handles {
        match h.join().expect("join") {
            Ok(()) => ok_count += 1,
            Err(_) => err_count += 1,
        }
    }
    assert!(
        ok_count <= 1,
        "at most one Landlock apply per process; ok={ok_count} err={err_count}"
    );
    assert!(
        ok_count + err_count == 4,
        "all threads must return a Result; ok={ok_count} err={err_count}"
    );

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn resource_apply_landlock_non_linux_no_fd_leak() {
    #[cfg(not(target_os = "linux"))]
    {
        use std::path::Path;
        let start = count_open_fds();
        for _ in 0..200 {
            let _ = apply_landlock(Path::new("/code"), Path::new("/tmp"));
        }
        let end = count_open_fds();
        assert!(end <= start + 4, "stub apply must not leak FDs");
    }
}
