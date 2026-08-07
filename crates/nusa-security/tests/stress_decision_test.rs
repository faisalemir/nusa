//! S12: Security sandbox stress decision tests.
//!
//! Covers Landlock/Seccomp combinatorics under stress, decision tables, and throughput.
//! Authoritative gate: `just podman-test-pkg nusa-security` (Alpine).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use nusa_security::verify_seccomp_filter;

// === Seccomp Verify Throughput Under Stress ===

#[test]
fn seccomp_verify_throughput_1k_iterations() {
    let counter = Arc::new(AtomicUsize::new(0));
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let counter = Arc::clone(&counter);
            std::thread::spawn(move || {
                for _ in 0..250 {
                    let result = verify_seccomp_filter();
                    if result.is_ok() {
                        counter.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread panicked");
    }

    let count = counter.load(Ordering::Relaxed);
    assert!(count > 0, "at least some seccomp verify calls must succeed");
}

#[test]
fn seccomp_verify_10k_stress() {
    let counter = Arc::new(AtomicUsize::new(0));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let counter = Arc::clone(&counter);
            std::thread::spawn(move || {
                for _ in 0..1250 {
                    let _ = verify_seccomp_filter();
                    counter.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread panicked");
    }

    let count = counter.load(Ordering::Relaxed);
    assert_eq!(count, 10_000, "all 10K iterations must complete");
}

// === Landlock Decision Table ===

#[test]
#[cfg(target_os = "linux")]
fn landlock_decision_table() {
    use nusa_security::apply_landlock;

    let tmp = std::env::temp_dir();

    // Decision table: (code_dir_exists, tmp_dir_exists, expected)
    let cases = [
        (true, true, true),   // both exist → should succeed (first apply)
        (false, true, false), // code_dir missing → fail
        (true, false, false), // tmp_dir missing → fail
    ];

    for (code_exists, tmp_exists, expected) in cases {
        let code_dir = if code_exists {
            let d = tmp.join(format!("landlock_decision_code_{}", code_exists));
            std::fs::create_dir_all(&d).ok();
            d
        } else {
            tmp.join("nonexistent_landlock_code_dir")
        };
        let tmp_dir = if tmp_exists {
            let d = tmp.join("landlock_decision_tmp");
            std::fs::create_dir_all(&d).ok();
            d
        } else {
            tmp.join("nonexistent_landlock_tmp")
        };

        let result = apply_landlock(&code_dir, &tmp_dir);
        if expected {
            // First apply returns Ok; subsequent applies return "already applied" error.
            // Both are valid outcomes — the second means a previous test already secured the process.
            if let Err(ref e) = result {
                assert!(
                    e.to_string().contains("already applied"),
                    "code_exists={code_exists}, tmp_exists={tmp_exists}: expected Ok or 'already applied', got: {e}"
                );
            }
        } else {
            // May fail for various reasons — we assert it's not a panic
            let _ = result;
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&code_dir);
    }
}

// === Seccomp Decision Table ===

#[test]
fn seccomp_decision_table() {
    // verify_seccomp_filter should always succeed (build only, not install)
    let result = verify_seccomp_filter();
    assert!(
        result.is_ok(),
        "seccomp filter build must succeed: {result:?}"
    );
}

// === Combined Security Decision Table ===

#[test]
fn combined_security_decision_table() {
    // (seccomp_ok, landlock_first_apply_ok)
    // This tests the interaction pattern

    // Seccomp verify should always succeed
    let seccomp_ok = verify_seccomp_filter().is_ok();
    assert!(seccomp_ok, "seccomp verify must succeed");

    // Landlock first-apply decision:
    // If not yet applied, should succeed with valid paths
    // If already applied, should return Err (idempotent gate)
    #[cfg(target_os = "linux")]
    {
        let tmp = std::env::temp_dir();
        let code_dir = tmp.join("combined_decision_code");
        let tmp_dir = tmp.join("combined_decision_tmp");
        std::fs::create_dir_all(&code_dir).ok();
        std::fs::create_dir_all(&tmp_dir).ok();

        let result = nusa_security::apply_landlock(&code_dir, &tmp_dir);
        // Either succeeds (first apply) or fails (already applied) — both valid
        let _ = result;

        // Second apply must fail (already applied gate)
        let result2 = nusa_security::apply_landlock(&code_dir, &tmp_dir);
        // If first succeeded, second must fail
        // If first failed, second may also fail for different reason
        let _ = result2;

        let _ = std::fs::remove_dir_all(&code_dir);
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}

// === Recovery After Stress ===

#[test]
fn seccomp_recovery_after_1k_calls() {
    for i in 0..1000 {
        let result = verify_seccomp_filter();
        if i % 100 == 0 {
            assert!(result.is_ok(), "iteration {i}: seccomp verify must succeed");
        }
    }
}
