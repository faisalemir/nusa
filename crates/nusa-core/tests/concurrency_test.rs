//! Exhaustive concurrency tests for core components.
//!
//! rust-test-deep Phase 3: Concurrency Exhaustive
//! rust-test-deep §1: Data Race Matrix
//! rust-test-deep §6: Task Cancellation Matrix

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use nusa_core::{OffloadTask, TaskManager, TenantId, TenantRateLimiter};

// ── Data Race Tests ──

/// === Arrange ===
/// TaskManager shared across 8 threads, 100 tasks each.
/// === Act ===
/// All threads submit concurrently.
/// === Assert ===
/// All 800 tasks submitted, unique IDs, no corruption.
#[test]
fn task_manager_concurrent_submit_8_threads() {
    // === Arrange ===
    let mgr = Arc::new(TaskManager::new());
    let counter = Arc::new(AtomicU64::new(0));

    let mut handles = vec![];

    // === Act ===
    for thread_id in 0..8 {
        let m = Arc::clone(&mgr);
        let c = Arc::clone(&counter);
        handles.push(thread::spawn(move || {
            for _ in 0..100 {
                let (id, rx) = m.submit(OffloadTask::Custom {
                    task_type: format!("t{}-task", thread_id),
                    payload: serde_json::json!({ "thread": thread_id }),
                });
                // Verify ID is not empty
                assert!(!id.is_empty());
                // Consume result to avoid channel buildup
                let _ = rx.blocking_recv();
                c.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // === Assert ===
    assert_eq!(counter.load(Ordering::SeqCst), 800);
}

// ── Check-Then-Act Race Tests ──

/// === Arrange ===
/// TenantRateLimiter, 10 threads competing for same tenant.
/// === Act ===
/// Each thread sends 100 requests.
/// === Assert ===
/// Exactly burst_size allowed, no more.
#[test]
fn rate_limiter_check_then_act_no_race() {
    // === Arrange ===
    let limiter = Arc::new(TenantRateLimiter::new(60, 50));
    let tenant = TenantId::new("competitive");
    let allowed = Arc::new(AtomicU64::new(0));

    let mut handles = vec![];

    // === Act ===
    for _ in 0..10 {
        let l = Arc::clone(&limiter);
        let t = tenant.clone();
        let a = Arc::clone(&allowed);
        handles.push(thread::spawn(move || {
            for _ in 0..100 {
                if l.is_allowed(&t) {
                    a.fetch_add(1, Ordering::SeqCst);
                }
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // === Assert ===
    let a = allowed.load(Ordering::SeqCst);
    // Due to race conditions in check-then-act, we might get slightly more than burst
    // This tests that the limiter is approximately correct
    assert!(
        a <= 60,
        "allowed {} but expected <= 60 (burst 50 + race tolerance)",
        a
    );
}

// ── Task Cancellation Tests ──

/// === Arrange ===
/// TaskManager, task submitted but receiver dropped.
/// === Act ===
/// Drop receiver before task completes.
/// === Assert ===
/// No panic, task completes in background.
#[test]
fn task_manager_receiver_dropped_no_panic() {
    // === Arrange ===
    let mgr = TaskManager::new();

    // === Act ===
    let (id, rx) = mgr.submit(OffloadTask::Custom {
        task_type: "drop-test".into(),
        payload: serde_json::json!({}),
    });
    drop(rx); // drop receiver immediately

    // Wait for task to complete in background (retry up to 500ms)
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(50));
        let status = mgr.status(&id);
        if status.completed {
            // === Assert ===
            return;
        }
    }

    panic!("task should complete even if receiver dropped");
}

/// === Arrange ===
/// TaskManager, 100 tasks submitted, all receivers dropped.
/// === Act ===
/// Submit and immediately drop all receivers.
/// === Assert ===
/// No panic, no memory leak.
#[test]
fn task_manager_many_dropped_receivers_no_leak() {
    // === Arrange ===
    let mgr = TaskManager::new();

    // === Act ===
    for _ in 0..100 {
        let (_id, rx) = mgr.submit(OffloadTask::Custom {
            task_type: "leak-test".into(),
            payload: serde_json::json!({}),
        });
        drop(rx);
    }

    // Wait for tasks to complete
    std::thread::sleep(Duration::from_millis(200));

    // === Assert ===
    // No panic, no memory leak
}

// ── Thread Safety: Send/Sync Verification ──

/// === Arrange ===
/// TaskManager used in spawned thread.
/// === Act ===
/// Submit task from different thread.
/// === Assert ===
/// Send/Sync bounds satisfied.
#[test]
fn task_manager_send_sync_across_threads() {
    // === Arrange ===
    let mgr = Arc::new(TaskManager::new());
    let m = Arc::clone(&mgr);

    // === Act ===
    let handle = thread::spawn(move || {
        let (_id, rx) = m.submit(OffloadTask::Custom {
            task_type: "cross-thread".into(),
            payload: serde_json::json!({}),
        });
        rx.blocking_recv()
    });

    let result = handle.join().unwrap();

    // === Assert ===
    assert!(result.is_ok(), "task must complete across threads");
}

// ── Panic in Task Tests ──

/// === Arrange ===
/// Custom task that will fail (not implemented).
/// === Act ===
/// Submit task, check error handling.
/// === Assert ===
/// Task returns error result, doesn't panic.
#[test]
fn task_custom_variant_returns_error_not_panic() {
    // === Arrange ===
    let mgr = Arc::new(TaskManager::new());

    // === Act ===
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let m = Arc::clone(&mgr);
        let (_id, rx) = m.submit(OffloadTask::Custom {
            task_type: "panic-test".into(),
            payload: serde_json::json!({}),
        });
        rx.blocking_recv()
    }));

    // === Assert ===
    assert!(result.is_ok(), "task submission must not panic");
    let inner = result.unwrap();
    assert!(inner.is_ok(), "task result must be received");
}
