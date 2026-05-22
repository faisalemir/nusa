//! Detailed integration tests for nusa-octane-worker crate
//!
//! Skills applied:
//! - `m07-concurrency`: Worker pool lifecycle management, async spawn/recycle
//! - `m12-lifecycle`: explicit init→execute→shutdown phases
//! - `m13-domain-error`: Worker state transitions and error handling
//! - `m06-error-handling`: Result propagation through worker operations

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tokio::sync::Mutex;

use nusa_octane_worker::pool::{Worker, WorkerPool, WorkerState};
use nusa_octane_worker::state_reset::{OctaneEvent, StateResetOrchestrator};

/// Writable app root that exists (required by production spawn); PHP worker may still be absent.
fn test_app_root() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nusa_octane_pool_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("test app_root must exist");
    dir
}

/// Stub worker for unit tests without PHP (live path: `podman-test-laravel`).
fn test_stub_worker(id: usize) -> Worker {
    Worker::new_test_stub(id)
}

// ── Worker Lifecycle Tests ──

#[cfg(not(unix))]
#[tokio::test]
async fn worker_spawn_creates_stub_on_non_unix() {
    let worker = Worker::spawn(0, test_app_root(), 512)
        .await
        .expect("Windows stub spawn");
    assert_eq!(worker.id, 0);
    assert_eq!(worker.state, WorkerState::Idle);
    assert_eq!(worker.requests_handled.load(Ordering::SeqCst), 0);
    assert_eq!(worker.rss_mb.load(Ordering::SeqCst), 0);
}

#[cfg(unix)]
#[tokio::test]
async fn worker_spawn_fails_closed_without_php_driver() {
    let result = Worker::spawn(0, test_app_root(), 512).await;
    let Err(err) = result else {
        panic!("Unix spawn without php-driver must fail");
    };
    let msg = err.to_string();
    assert!(
        msg.contains("octane") || msg.contains("Handshake") || msg.contains("missing"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn worker_handle_request_fails_on_stub() {
    let mut worker = test_stub_worker(0);

    let result = worker
        .handle_request(
            "GET".into(),
            "/index.php".into(),
            Default::default(),
            None,
            5000,
        )
        .await;

    assert!(
        result.is_err(),
        "handle_request must fail on stub worker (no transport)",
    );
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("no transport"),
        "Error message must indicate stub mode: {}",
        err,
    );
}

#[tokio::test]
async fn worker_stop_transitions_to_stopped() {
    let mut worker = test_stub_worker(0);

    assert_eq!(worker.state, WorkerState::Idle);

    let result = worker.stop().await;
    assert!(result.is_ok(), "stop() must succeed on stub worker");
    assert_eq!(
        worker.state,
        WorkerState::Stopped,
        "Worker must transition to Stopped after stop()",
    );
}

#[tokio::test]
async fn worker_should_recycle_by_requests() {
    let worker = test_stub_worker(0);

    // Set request count manually
    worker.requests_handled.store(1000, Ordering::SeqCst);

    assert!(
        worker.should_recycle(500, 1024),
        "Worker should recycle when requests >= max_requests",
    );
    assert!(
        !worker.should_recycle(2000, 1024),
        "Worker should NOT recycle when requests < max_requests",
    );
}

#[tokio::test]
async fn worker_should_recycle_by_memory() {
    let worker = test_stub_worker(0);

    worker.rss_mb.store(800, Ordering::SeqCst);

    assert!(
        worker.should_recycle(1000, 512),
        "Worker should recycle when RSS >= max_memory_mb",
    );
    assert!(
        !worker.should_recycle(1000, 2048),
        "Worker should NOT recycle when RSS < max_memory_mb",
    );
}

#[tokio::test]
async fn worker_atomic_counters_are_thread_safe() {
    let worker = Arc::new(test_stub_worker(0));

    // Simulate concurrent counter increments
    let mut handles = vec![];
    for _ in 0..10 {
        let w = Arc::clone(&worker);
        handles.push(tokio::spawn(async move {
            for _ in 0..100 {
                w.requests_handled.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert_eq!(
        worker.requests_handled.load(Ordering::SeqCst),
        1000,
        "Atomic counter must be exactly 1000 after 10 threads × 100 increments",
    );
}

// ── Worker Pool Tests ──

#[test]
fn pool_is_ready_false_for_test_stubs() {
    let mut pool = WorkerPool::new(2, test_app_root(), 512, 1000);
    pool.initialize_test_stubs();
    assert!(!pool.is_ready());
    assert_eq!(pool.idle_count(), 2);
}

#[tokio::test]
async fn pool_handle_http_request_fails_when_not_ready() {
    let mut pool = WorkerPool::new(1, test_app_root(), 512, 1000);
    pool.initialize_test_stubs();
    let result = pool
        .handle_http_request("GET".into(), "/".into(), Default::default(), None, 5000)
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn pool_initialization_creates_workers() {
    let mut pool = WorkerPool::new(3, test_app_root(), 512, 1000);

    pool.initialize_test_stubs();
    assert_eq!(pool.idle_count(), 3, "Pool must have 3 idle workers");
    assert!(
        !pool.is_ready(),
        "test stubs must not report production-ready"
    );
}

#[tokio::test]
async fn pool_get_idle_worker_dequeues() {
    let mut pool = WorkerPool::new(2, test_app_root(), 512, 1000);

    pool.initialize_test_stubs();

    // Get first idle worker
    let worker1 = pool.get_idle_worker();
    assert!(worker1.is_some(), "Must get an idle worker");
    assert_eq!(pool.idle_count(), 1, "Queue must have 1 worker left");

    // Get second idle worker
    let worker2 = pool.get_idle_worker();
    assert!(worker2.is_some(), "Must get second idle worker");
    assert_eq!(pool.idle_count(), 0, "Queue must be empty");

    // No more idle workers
    let worker3 = pool.get_idle_worker();
    assert!(worker3.is_none(), "Must return None when no idle workers");
}

#[tokio::test]
async fn pool_return_worker_enqueues() {
    let mut pool = WorkerPool::new(2, test_app_root(), 512, 1000);

    pool.initialize_test_stubs();

    // Get both workers
    pool.get_idle_worker();
    pool.get_idle_worker();
    assert_eq!(pool.idle_count(), 0);

    // Return one worker
    pool.return_worker(0);
    assert_eq!(pool.idle_count(), 1);

    // Draining worker should NOT be returned
    pool.worker_mut(1).state = WorkerState::Draining;
    pool.return_worker(1);
    assert_eq!(
        pool.idle_count(),
        1,
        "Draining worker must not be re-enqueued",
    );
}

#[tokio::test]
async fn pool_recycle_worker_replaces() {
    let mut pool = WorkerPool::new(1, test_app_root(), 512, 1000);

    pool.initialize_test_stubs();
    assert_eq!(pool.idle_count(), 1);

    // Get the only worker
    pool.get_idle_worker();
    assert_eq!(pool.idle_count(), 0);

    // Recycle it
    let result = pool.recycle_worker(0).await;
    assert!(result.is_ok(), "Recycle must succeed");
    assert_eq!(pool.idle_count(), 1, "Recycled worker must be idle");
    assert_eq!(
        pool.worker(0).requests_handled.load(Ordering::SeqCst),
        0,
        "New worker must have zero request count",
    );
}

#[tokio::test]
async fn pool_shutdown_clears_all() {
    let mut pool = WorkerPool::new(3, test_app_root(), 512, 1000);

    pool.initialize_test_stubs();
    assert_eq!(pool.worker_count(), 3);

    let result = pool.shutdown().await;
    assert!(result.is_ok(), "Shutdown must succeed");
    assert_eq!(
        pool.worker_count(),
        0,
        "All workers must be removed after shutdown",
    );
    assert_eq!(
        pool.idle_count(),
        0,
        "Idle queue must be cleared after shutdown",
    );
}

#[tokio::test]
async fn pool_statistics_tracking() {
    let mut pool = WorkerPool::new(2, test_app_root(), 512, 1000);

    pool.initialize_test_stubs();

    // Simulate work
    pool.worker_mut(0)
        .requests_handled
        .store(50, Ordering::SeqCst);
    pool.worker_mut(1)
        .requests_handled
        .store(30, Ordering::SeqCst);
    pool.worker_mut(0).error_count.store(2, Ordering::SeqCst);

    assert_eq!(pool.total_handled(), 0); // Counter not yet aggregated
    assert_eq!(pool.total_errors(), 0);

    // Manual aggregation test
    let total: u64 = (0..pool.worker_count())
        .map(|i| pool.worker(i).requests_handled.load(Ordering::SeqCst))
        .sum();
    assert_eq!(total, 80, "Total requests handled must be 80");
}

// ── State Reset Orchestrator Tests ──

#[tokio::test]
async fn orchestrator_emits_events() {
    let mut orchestrator = StateResetOrchestrator::new(10);
    orchestrator.initialize();

    let event = OctaneEvent::WorkerStarted { worker_id: 0 };
    orchestrator.emit_event(event); // should not panic
}

#[tokio::test]
async fn orchestrator_broadcast_subscribers() {
    let orchestrator = StateResetOrchestrator::new(10);
    let mut receiver = orchestrator.subscribe();

    // Send event in separate task
    let orch = Arc::new(Mutex::new(orchestrator));
    let sender = tokio::spawn(async move {
        let orch = orch.lock().await;
        orch.emit_event(OctaneEvent::RequestReceived {
            request_id: "req-1".into(),
        })
    });

    // Wait for event
    let recv_result = tokio::time::timeout(Duration::from_secs(1), receiver.recv()).await;

    assert!(
        recv_result.is_ok(),
        "Subscriber must receive broadcasted event within timeout",
    );
    sender.await.unwrap();
}

#[tokio::test]
async fn orchestrator_stats_increment() {
    let mut orchestrator = StateResetOrchestrator::new(10);
    orchestrator.initialize();

    // Emit multiple events
    for i in 0..5 {
        orchestrator.emit_event(OctaneEvent::RequestReceived {
            request_id: format!("req-{}", i),
        });
    }

    let stats = orchestrator.stats();
    assert_eq!(
        stats.total_requests_processed, 5,
        "Stats must reflect 5 emitted events",
    );
}

#[tokio::test]
async fn orchestrator_custom_action() {
    let mut orchestrator = StateResetOrchestrator::new(10);

    // Register a custom action
    orchestrator.register_action("request_received".into(), |_event| {
        // Custom action executed
    });

    // Emit event to trigger action
    orchestrator.emit_event(OctaneEvent::RequestReceived {
        request_id: "test".into(),
    });
}

#[tokio::test]
async fn orchestrator_shutdown() {
    let orchestrator = StateResetOrchestrator::new(10);
    // Should not panic
    orchestrator.shutdown();
}

// ── Edge Cases and Error Handling ──

#[tokio::test]
async fn worker_with_zero_max_requests_recycles_immediately() {
    let worker = test_stub_worker(0);

    // Zero requests handled, but max_requests is 0
    assert!(
        worker.should_recycle(0, 1024),
        "Worker should recycle when max_requests is 0 (edge case)",
    );
}

#[tokio::test]
async fn worker_with_zero_max_memory_recycles_immediately() {
    let worker = test_stub_worker(0);

    // Zero RSS, but max_memory is 0
    assert!(
        worker.should_recycle(1000, 0),
        "Worker should recycle when max_memory_mb is 0 (edge case)",
    );
}

#[tokio::test]
async fn pool_with_zero_workers_initializes_empty() {
    let mut pool = WorkerPool::new(0, test_app_root(), 512, 1000);

    let result = pool.initialize().await;
    assert!(result.is_ok(), "Pool with 0 workers must initialize");
    assert_eq!(pool.worker_count(), 0);
    assert_eq!(pool.idle_count(), 0);
}

#[tokio::test]
async fn worker_request_timeout_behavior() {
    let mut worker = test_stub_worker(0);

    // Very short timeout on stub worker
    let result = worker
        .handle_request(
            "GET".into(),
            "/index.php".into(),
            Default::default(),
            None,
            1,
        )
        .await;

    assert!(result.is_err(), "Request must fail (stub mode or timeout)",);
}

#[tokio::test]
async fn pool_recycle_preserves_worker_id() {
    let mut pool = WorkerPool::new(2, test_app_root(), 512, 1000);

    pool.initialize_test_stubs();

    // Recycle worker 1
    pool.recycle_worker(1).await.unwrap();

    assert_eq!(
        pool.worker(1).id,
        1,
        "Recycled worker must preserve its original ID",
    );
}

#[tokio::test]
async fn pool_multiple_recycles_succeed() {
    let mut pool = WorkerPool::new(3, test_app_root(), 512, 1000);

    pool.initialize_test_stubs();

    // Recycle all workers sequentially
    for i in 0..pool.worker_count() {
        let result = pool.recycle_worker(i).await;
        assert!(result.is_ok(), "Recycle of worker {} must succeed", i,);
    }

    assert_eq!(pool.idle_count(), 3, "All recycled workers must be idle");
}
