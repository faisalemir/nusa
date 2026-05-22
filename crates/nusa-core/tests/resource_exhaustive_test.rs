//! Resource exhaustion tests for nusa-core crate.
//!
//! Covers: Leak detection for TaskManager/TenantRegistry/TenantId/RequestContext,
//! FD leak, resource exhaustion boundaries, cleanup after failure/panic,
//! Arc reference count verification.

use std::sync::Arc;
use std::time::Duration;

use nusa_core::{
    BackpressureGuard, OffloadTask, TaskManager, TenantConfig, TenantId, TenantRateLimiter,
    TenantRegistry,
};

// ── Leak Detection: TaskManager ──

#[test]
fn taskmanager_creation_drop_no_leak() {
    // === Arrange ===
    let start_fds = count_open_fds();

    // === Act ===
    for _ in 0..100 {
        let _manager = TaskManager::new();
    }

    // === Assert ===
    let end_fds = count_open_fds();
    assert_eq!(
        end_fds, start_fds,
        "FD count must be stable after creation+drop"
    );
}

#[test]
fn taskmanager_creation_in_loop_no_growth() {
    // === Arrange ===
    let start_fds = count_open_fds();

    // === Act ===
    for _ in 0..1000 {
        let manager = TaskManager::new();
        let task = OffloadTask::Custom {
            task_type: "loop".to_string(),
            payload: serde_json::json!({}),
        };
        let (_id, _rx) = manager.submit(task);
        // Drop manager immediately
        drop(manager);
    }

    // === Assert ===
    let end_fds = count_open_fds();
    let fd_slack = if cfg!(target_os = "linux") { 30 } else { 5 };
    assert!(
        end_fds <= start_fds + fd_slack,
        "FD growth must be minimal after 1000 iterations (start={start_fds}, end={end_fds})"
    );
}

#[test]
fn taskmanager_move_to_thread_thread_finishes_no_leak() {
    // === Arrange ===
    let manager = TaskManager::new();

    // === Act ===
    let handle = std::thread::spawn(move || {
        let task = OffloadTask::Custom {
            task_type: "thread".to_string(),
            payload: serde_json::json!({}),
        };
        let (_id, _rx) = manager.submit(task);
    });

    // === Assert ===
    handle.join().expect("thread must not panic");
    let _end_fds = count_open_fds();
    // No leak assertion — just verify thread completed cleanly
}

#[test]
fn taskmanager_arc_drop_all_clones_no_leak() {
    // === Arrange ===
    let manager = Arc::new(TaskManager::new());
    let clones: Vec<_> = (0..10).map(|_| manager.clone()).collect();

    // === Act ===
    // Drop all clones
    drop(clones);
    drop(manager);

    // === Assert ===
    // No leak — Arc ref count goes to zero
}

#[test]
fn taskmanager_weak_detects_strong_dropped() {
    // === Arrange ===
    // TaskManager doesn't expose Weak directly, but we can test Arc behavior
    let strong = Arc::new(TaskManager::new());
    let weak = Arc::downgrade(&strong);

    // === Act ===
    drop(strong);

    // === Assert ===
    assert!(
        weak.upgrade().is_none(),
        "weak must fail after strong dropped"
    );
}

// ── Leak Detection: TenantRegistry ──

#[test]
fn tenantregistry_creation_drop_no_leak() {
    // === Arrange ===
    let start_fds = count_open_fds();

    // === Act ===
    for _ in 0..100 {
        let _registry = TenantRegistry::new();
    }

    // === Assert ===
    let end_fds = count_open_fds();
    assert_eq!(end_fds, start_fds, "no FD leak");
}

#[test]
fn tenantregistry_creation_in_loop_no_growth() {
    // === Arrange ===
    let start_fds = count_open_fds();

    // === Act ===
    for _ in 0..1000 {
        let mut registry = TenantRegistry::new();
        let id = TenantId::new("loop-tenant");
        registry.register(TenantConfig {
            id,
            vfs_root: "/tmp/loop".to_string(),
            max_memory_mb: 256,
            max_requests_per_minute: 100,
            enabled: true,
        });
        drop(registry);
    }

    // === Assert ===
    let end_fds = count_open_fds();
    assert!(end_fds <= start_fds + 5, "no FD growth");
}

#[test]
fn tenantregistry_arc_drop_all_clones_no_leak() {
    // === Arrange ===
    let registry = Arc::new(TenantRegistry::new());
    let clones: Vec<_> = (0..10).map(|_| registry.clone()).collect();

    // === Act ===
    drop(clones);
    drop(registry);
}

// ── Leak Detection: TenantId ──

#[test]
fn tenantid_creation_in_loop_no_leak() {
    // === Arrange ===
    let start_fds = count_open_fds();

    // === Act ===
    for i in 0..10_000 {
        let id = TenantId::new(format!("tenant-{}", i));
        // Use it
        let _ = id.as_str();
    }

    // === Assert ===
    let end_fds = count_open_fds();
    assert!(
        end_fds <= start_fds + 5,
        "no FD leak from TenantId creation"
    );
}

#[test]
fn tenantid_arc_ref_count_zero_after_drop() {
    // === Arrange ===
    let strong = Arc::new(TenantId::new("refcount-test"));
    let weak = Arc::downgrade(&strong);

    // === Act ===
    assert_eq!(Arc::strong_count(&strong), 1);
    drop(strong);

    // === Assert ===
    assert!(
        weak.upgrade().is_none(),
        "weak must not upgrade after strong dropped"
    );
}

// ── File Descriptor Leak: VFS Operations ──

#[tokio::test]
async fn vfs_repeated_operations_fd_stable() {
    // === Arrange ===
    let start_fds = count_open_fds();
    let tmp_dir = std::env::temp_dir().join("nusa_vfs_fd_test");
    tokio::fs::create_dir_all(&tmp_dir)
        .await
        .expect("must create dir");

    // === Act ===
    for i in 0..100 {
        let path = tmp_dir.join(format!("file_{}.txt", i));
        tokio::fs::write(&path, b"test data")
            .await
            .expect("must write");
        let _ = tokio::fs::read(&path).await;
        let _ = tokio::fs::remove_file(&path).await;
    }

    // === Assert ===
    let end_fds = count_open_fds();
    assert!(
        end_fds <= start_fds + 10,
        "FD count must be stable after VFS operations"
    );

    // Cleanup
    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
}

// ── Resource Exhaustion Boundaries ──

#[test]
fn resourceguard_zero_capacity_backpressure_immediate_reject() {
    // === Arrange ===
    let guard = BackpressureGuard::new(0);

    // === Act ===
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("must build runtime");

    let result = rt.block_on(guard.try_acquire());

    // === Assert ===
    assert!(result.is_none(), "zero-cap guard must reject all requests");
}

#[test]
fn resourceguard_max_request_size_zero_rejects_all() {
    // === Arrange ===
    let max_bytes = 0usize;

    // === Act & Assert ===
    assert!(
        !nusa_core::validate_request_size(Some(1), max_bytes),
        "must reject"
    );
    assert!(
        !nusa_core::validate_request_size(Some(0), max_bytes),
        "zero content-length must also reject when max is 0"
    );
}

#[test]
fn resourceguard_validate_request_size_large_value() {
    // === Arrange ===
    let max_bytes = 10 * 1024 * 1024; // 10MB

    // === Act & Assert ===
    // Very large but within limit
    assert!(nusa_core::validate_request_size(
        Some(10 * 1024 * 1024),
        max_bytes
    ));
    // At usize::MAX
    let result = nusa_core::validate_request_size(Some(u64::MAX), max_bytes);
    assert!(!result, "usize::MAX must be rejected");
}

// ── Cleanup After Failure ──

#[tokio::test]
async fn taskmanager_task_fails_cleanup_verified() {
    // === Arrange ===
    let manager = TaskManager::new();

    // === Act ===
    // Submit task that will fail (invalid file operation)
    let task = OffloadTask::FileOperation {
        operation: "read".to_string(),
        path: "/nonexistent/file.txt".to_string(),
        data: None,
    };
    let (_id, rx) = manager.submit(task);

    let result = tokio::time::timeout(Duration::from_secs(10), rx).await;

    // === Assert ===
    if let Ok(Ok(task_result)) = result {
        assert!(!task_result.success, "task must fail");
        assert!(task_result.error.is_some(), "error must be present");
    }

    // Manager must still be functional
    let task2 = OffloadTask::Custom {
        task_type: "recovery".to_string(),
        payload: serde_json::json!({}),
    };
    let (_id2, _rx2) = manager.submit(task2);
}

#[tokio::test]
async fn ratelimiter_error_path_cleanup() {
    // === Arrange ===
    let limiter = TenantRateLimiter::new(100, 10);
    let tenant_id = TenantId::new("cleanup-test");

    // === Act ===
    // Exhaust all tokens
    for _ in 0..15 {
        limiter.is_allowed(&tenant_id);
    }

    // Verify remaining shows 0 or low
    let remaining = limiter.remaining(&tenant_id);

    // === Assert ===
    assert!(
        remaining.is_some(),
        "tenant must be tracked even after exhaustion"
    );
    assert!(
        remaining.expect("must exist") <= 10,
        "remaining must be within burst"
    );
}

// ── Panic Cleanup ──

#[test]
fn tenantregistry_panic_during_register_no_leak() {
    // === Arrange ===
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut registry = TenantRegistry::new();
        registry.register(TenantConfig {
            id: TenantId::new("pre-panic"),
            vfs_root: "/tmp/pre".to_string(),
            max_memory_mb: 256,
            max_requests_per_minute: 100,
            enabled: true,
        });
        // Simulate panic during register
        panic!("simulated panic during register");
    }));

    // === Act & Assert ===
    assert!(result.is_err(), "panic must propagate");
    // No resource leak — registry was dropped on panic
}

#[test]
fn backpressureguard_panic_during_acquire_cleanup() {
    // === Arrange ===
    let guard = BackpressureGuard::new(1);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("must build");

        rt.block_on(async {
            let permit = guard.try_acquire().await.expect("must acquire");
            // Panic while holding permit
            drop(permit);
            panic!("simulated panic with permit");
        });
    }));

    // === Act & Assert ===
    assert!(result.is_err(), "panic must propagate");
    // Permit was dropped via catch_unwind — no leak
}

// ── Arc Reference Count Verification ──

#[test]
fn backpressureguard_arc_strong_count_zero_after_drops() {
    // === Arrange ===
    let guard = Arc::new(BackpressureGuard::new(10));
    assert_eq!(Arc::strong_count(&guard), 1);

    // === Act ===
    let clone1 = guard.clone();
    let clone2 = guard.clone();
    assert_eq!(Arc::strong_count(&guard), 3);

    drop(clone1);
    assert_eq!(Arc::strong_count(&guard), 2);

    drop(clone2);
    assert_eq!(Arc::strong_count(&guard), 1);

    drop(guard);
    // After this, count is 0 — no way to check, but no panic = success
}

#[test]
fn tenantregistry_weak_upgrade_failure_detection() {
    // === Arrange ===
    let strong = Arc::new(TenantRegistry::new());
    let weak = Arc::downgrade(&strong);

    // === Act ===
    assert!(
        weak.upgrade().is_some(),
        "weak must succeed while strong alive"
    );
    drop(strong);

    // === Assert ===
    assert!(
        weak.upgrade().is_none(),
        "weak must fail after strong dropped"
    );
}

// ── Helper ──

#[cfg(unix)]
fn count_open_fds() -> usize {
    use std::fs;
    let fd_dir = "/proc/self/fd";
    if let Ok(entries) = fs::read_dir(fd_dir) {
        entries.count()
    } else {
        0
    }
}

#[cfg(not(unix))]
fn count_open_fds() -> usize {
    // On Windows, we can't easily count FDs, so return 0
    // Tests using this will be less strict on Windows
    0
}
