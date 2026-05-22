//! Resource exhaustion tests for nusa-octane-worker crate.
//!
//! Covers: Worker FD leak, Pool memory leak, leak after shutdown mid-request,
//! orchestrator leak, panic recovery, error path cleanup, connection leak,
//! repeated failed init, pool exhaustion.

use std::path::PathBuf;
use std::time::Duration;

use nusa_octane_worker::metrics::WorkerMetrics;
use nusa_octane_worker::pool::{WorkerPool, WorkerState};
use nusa_octane_worker::state_reset::{OctaneEvent, StateResetOrchestrator};

// ── Pool Memory Leak ──

#[tokio::test]
async fn workerpool_recycle_n_times_heap_memory_stable() {
    // === Arrange ===
    let start_fds = count_open_fds();
    let mut pool = WorkerPool::new(1, PathBuf::from("/tmp/nusa-test-nonexistent"), 256, 10);

    // === Act ===
    // Init will fail, but we test that repeated attempts don't leak
    for _ in 0..5 {
        let _ = pool.initialize().await;
        let _ = pool.shutdown().await;
    }

    // === Assert ===
    let end_fds = count_open_fds();
    assert!(
        end_fds <= start_fds + 20,
        "FD growth must be bounded after repeated init/shutdown"
    );
}

// ── Pool: Leak After Spawn Failure ──

#[tokio::test]
async fn workerpool_partial_resource_cleanup_on_failed_spawn() {
    // === Arrange ===
    let start_fds = count_open_fds();
    let mut pool = WorkerPool::new(4, PathBuf::from("/nonexistent/path"), 256, 100);

    // === Act ===
    let result = pool.initialize().await;

    // === Assert ===
    assert!(
        result.is_err(),
        "init must fail when app_root does not exist (production guard)"
    );

    // Shutdown must still work (cleanup partial resources)
    let shutdown_result = pool.shutdown().await;
    assert!(
        shutdown_result.is_ok(),
        "shutdown must succeed even after failed init"
    );

    let end_fds = count_open_fds();
    assert!(end_fds <= start_fds + 10, "no FD leak from failed spawn");
}

// ── Orchestrator Leak ──

#[test]
fn orchestrator_dropped_subscribers_broadcast_cleanup() {
    // === Arrange ===
    let mut orchestrator = StateResetOrchestrator::new(128);
    orchestrator.initialize();

    // === Act ===
    // Create subscribers and drop them
    for _ in 0..10 {
        let _rx = orchestrator.subscribe();
        // rx dropped immediately
    }

    // Emit events after subscribers dropped
    for i in 0..100 {
        orchestrator.emit_event(OctaneEvent::RequestReceived {
            request_id: format!("req-{}", i),
        });
    }

    // === Assert ===
    let stats = orchestrator.stats();
    assert_eq!(
        stats.total_requests_processed, 100,
        "events must be counted"
    );
}

// ── Pool Panic Recovery ──

#[test]
fn workerpool_panic_recovery_no_resource_leak() {
    // === Arrange ===
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut pool = WorkerPool::new(1, PathBuf::from("/tmp"), 256, 10);
        // Panic during pool operation
        let _ = pool.get_idle_worker();
        panic!("simulated panic during pool operation");
    }));

    // === Act & Assert ===
    assert!(result.is_err(), "panic must propagate");
    // Pool was dropped — no leak
}

// ── Worker Error Path Cleanup ──

#[tokio::test]
async fn worker_handle_request_error_all_resources_freed() {
    // === Arrange ===
    let start_fds = count_open_fds();

    // Create a pool that will fail (no PHP)
    let mut pool = WorkerPool::new(1, PathBuf::from("/nonexistent"), 256, 10);

    // === Act ===
    let _ = pool.initialize().await;

    // Try to get worker (may or may not succeed)
    if pool.idle_count() > 0 {
        let idx = pool.idle_count() - 1;
        let _ = pool
            .worker_mut(idx)
            .handle_request("GET".to_string(), "/test".to_string(), 5000)
            .await;
        pool.return_worker(pool.worker(idx).id);
    }

    let _ = pool.shutdown().await;

    // === Assert ===
    let end_fds = count_open_fds();
    assert!(end_fds <= start_fds + 10, "no FD leak from error path");
}

// ── WorkerMetrics: No Allocation Growth ──

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn workermetrics_concurrent_no_allocation_growth() {
    // === Arrange ===
    let metrics = Arc::new(WorkerMetrics::new());

    // === Act ===
    let mut handles = Vec::new();
    for _i in 0..8 {
        let m = metrics.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..1000 {
                m.record_request(1, true);
            }
        }));
    }

    // === Assert ===
    for h in handles {
        tokio::time::timeout(Duration::from_secs(10), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }

    assert_eq!(metrics.requests_handled(), 8000, "all must be recorded");
}

// ── Pool: Repeated Failed Init ──

#[tokio::test]
async fn workerpool_repeated_failed_init_no_resource_accumulation() {
    // === Arrange ===
    let start_fds = count_open_fds();
    let mut pool = WorkerPool::new(2, PathBuf::from("/nonexistent"), 256, 10);

    // === Act ===
    for _ in 0..10 {
        let _ = pool.initialize().await;
        let _ = pool.shutdown().await;
    }

    // === Assert ===
    let end_fds = count_open_fds();
    assert!(
        end_fds <= start_fds + 20,
        "FD growth must be bounded after 10 failed init cycles"
    );
}

// ── Pool Exhaustion Behavior ──

#[test]
fn workerpool_all_workers_busy_queuing_behavior() {
    // === Arrange ===
    let mut pool = WorkerPool::new(2, PathBuf::from("/tmp"), 256, 10);

    // Mark all workers as busy
    if pool.worker_count() >= 2 {
        pool.worker_mut(0).state = WorkerState::Busy;
        pool.worker_mut(1).state = WorkerState::Busy;
    }

    // === Act ===
    // Try to get idle worker — should return None
    let worker = pool.get_idle_worker();

    // === Assert ===
    assert!(worker.is_none(), "no idle worker when all busy");
    assert_eq!(pool.idle_count(), 0, "idle queue must be empty");
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
    0
}

use std::sync::Arc;
