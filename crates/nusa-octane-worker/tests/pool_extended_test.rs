//! Extended stress tests for nusa-octane-worker pool.
//!
//! Covers: pool boundary values, concurrent operations, worker lifecycle edge cases.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tokio::sync::Mutex;

use nusa_octane_worker::pool::{Worker, WorkerPool, WorkerState};

// ── Pool Boundary Values ──

#[tokio::test]
async fn pool_single_worker() {
    let mut pool = WorkerPool::new(1, PathBuf::from("/tmp/test"), 512, 1000);
    let result = pool.initialize().await;
    assert!(result.is_ok());
    assert_eq!(pool.worker_count(), 1);
    assert_eq!(pool.idle_count(), 1);
}

#[tokio::test]
async fn pool_many_workers() {
    let mut pool = WorkerPool::new(50, PathBuf::from("/tmp/test"), 512, 1000);
    let result = pool.initialize().await;
    assert!(result.is_ok());
    assert_eq!(pool.worker_count(), 50);
    assert_eq!(pool.idle_count(), 50);
}

// ── Worker Lifecycle Edge Cases ──

#[tokio::test]
async fn worker_stop_idempotent() {
    let mut worker = Worker::spawn(0, PathBuf::from("/tmp/test"), 512)
        .await
        .unwrap();

    assert!(worker.stop().await.is_ok());
    assert_eq!(worker.state, WorkerState::Stopped);

    // Second stop should also succeed (idempotent)
    assert!(worker.stop().await.is_ok());
    assert_eq!(worker.state, WorkerState::Stopped);
}

#[tokio::test]
async fn worker_should_recycle_exact_boundary() {
    let worker = Worker::spawn(0, PathBuf::from("/tmp/test"), 512)
        .await
        .unwrap();

    // Exactly at max_requests
    worker.requests_handled.store(1000, Ordering::SeqCst);
    assert!(worker.should_recycle(1000, 2048));

    // One below max_requests
    worker.requests_handled.store(999, Ordering::SeqCst);
    assert!(!worker.should_recycle(1000, 2048));

    // Exactly at max_memory
    worker.rss_mb.store(512, Ordering::SeqCst);
    assert!(worker.should_recycle(1000, 512));

    // One below max_memory
    worker.rss_mb.store(511, Ordering::SeqCst);
    assert!(!worker.should_recycle(1000, 512));
}

#[tokio::test]
async fn worker_stop_transitions_from_busy() {
    let mut worker = Worker::spawn(0, PathBuf::from("/tmp/test"), 512)
        .await
        .unwrap();

    worker.state = WorkerState::Busy;
    assert!(worker.stop().await.is_ok());
    assert_eq!(worker.state, WorkerState::Stopped);
}

// ── Pool Stress ──

#[tokio::test]
async fn pool_rapid_initialize_shutdown() {
    for _ in 0..10 {
        let mut pool = WorkerPool::new(2, PathBuf::from("/tmp/test"), 512, 1000);
        pool.initialize().await.unwrap();
        pool.shutdown().await.unwrap();
        assert_eq!(pool.worker_count(), 0);
    }
}

#[tokio::test]
async fn pool_concurrent_get_idle_and_return() {
    let pool = Arc::new(Mutex::new(WorkerPool::new(4, PathBuf::from("/tmp/test"), 512, 1000)));

    pool.lock().await.initialize().await.unwrap();

    // Get all workers concurrently — count how many succeed
    let success_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut handles = vec![];

    for _ in 0..4 {
        let p = pool.clone();
        let sc = success_count.clone();
        handles.push(tokio::spawn(async move {
            let mut pool = p.lock().await;
            if pool.get_idle_worker().is_some() {
                sc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let got_count = success_count.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(got_count, 4, "all 4 workers should be gotten");
}

#[tokio::test]
async fn pool_recycle_multiple_times_same_worker() {
    let mut pool = WorkerPool::new(1, PathBuf::from("/tmp/test"), 512, 1000);
    pool.initialize().await.unwrap();

    for i in 0..5 {
        let result = pool.recycle_worker(0).await;
        assert!(result.is_ok(), "recycle {} must succeed", i);
        assert_eq!(pool.idle_count(), 1);
        assert_eq!(pool.worker(0).requests_handled.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn pool_shutdown_after_recycle() {
    let mut pool = WorkerPool::new(3, PathBuf::from("/tmp/test"), 512, 1000);
    pool.initialize().await.unwrap();

    pool.recycle_worker(0).await.unwrap();
    pool.recycle_worker(1).await.unwrap();

    pool.shutdown().await.unwrap();
    assert_eq!(pool.worker_count(), 0);
}

// ── Orchestrator Event Ordering ──

#[tokio::test]
async fn orchestrator_out_of_order_events() {
    use nusa_octane_worker::state_reset::{OctaneEvent, StateResetOrchestrator};

    let mut orchestrator = StateResetOrchestrator::new(128);
    orchestrator.initialize();

    // Emit events out of order
    orchestrator.emit_event(OctaneEvent::RequestTerminated {
        request_id: "req-2".to_string(),
        status: 200,
    });
    orchestrator.emit_event(OctaneEvent::RequestReceived {
        request_id: "req-1".to_string(),
    });
    orchestrator.emit_event(OctaneEvent::WorkerStopping { worker_id: 0 });

    // All should be processed regardless of order
    let stats = orchestrator.stats();
    assert_eq!(stats.total_requests_processed, 1);
    assert_eq!(stats.total_resets_performed, 1);
    assert_eq!(stats.total_worker_stops, 1);
}

#[tokio::test]
async fn orchestrator_duplicate_events() {
    use nusa_octane_worker::state_reset::{OctaneEvent, StateResetOrchestrator};

    let mut orchestrator = StateResetOrchestrator::new(128);
    orchestrator.initialize();

    // Emit same event multiple times
    for _ in 0..5 {
        orchestrator.emit_event(OctaneEvent::RequestReceived {
            request_id: "req-1".to_string(),
        });
    }

    let stats = orchestrator.stats();
    assert_eq!(stats.total_requests_processed, 5);
}

#[tokio::test]
async fn orchestrator_multiple_actions_same_event() {
    use nusa_octane_worker::state_reset::{OctaneEvent, StateResetOrchestrator};
    use std::sync::atomic::{AtomicU64, Ordering};

    let counter = Arc::new(AtomicU64::new(0));
    let mut orchestrator = StateResetOrchestrator::new(128);

    let c1 = counter.clone();
    let c2 = counter.clone();
    let c3 = counter.clone();

    orchestrator.register_action("request_received".to_string(), move |_| {
        c1.fetch_add(1, Ordering::SeqCst);
    });
    orchestrator.register_action("request_received".to_string(), move |_| {
        c2.fetch_add(1, Ordering::SeqCst);
    });
    orchestrator.register_action("request_received".to_string(), move |_| {
        c3.fetch_add(1, Ordering::SeqCst);
    });

    orchestrator.emit_event(OctaneEvent::RequestReceived {
        request_id: "req-1".to_string(),
    });

    // All three actions should execute
    assert_eq!(counter.load(Ordering::SeqCst), 3);
}

// ── Worker handle_request with Various Combos ──

#[tokio::test]
async fn worker_handle_request_stub_error_message() {
    let mut worker = Worker::spawn(0, PathBuf::from("/tmp/test"), 512)
        .await
        .unwrap();

    let result = worker
        .handle_request("GET".into(), "/".into(), 5000)
        .await;

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("no transport"));
    assert!(err_msg.contains("0")); // worker id
}

#[tokio::test]
async fn worker_handle_request_various_methods() {
    // On stub workers, all methods should fail the same way
    let methods = ["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD"];

    for method in methods {
        let mut worker = Worker::spawn(0, PathBuf::from("/tmp/test"), 512)
            .await
            .unwrap();

        let result = worker
            .handle_request(method.to_string(), "/test".into(), 5000)
            .await;

        assert!(result.is_err(), "method {} should fail on stub", method);
    }
}

#[tokio::test]
async fn worker_handle_request_various_uris() {
    let long_uri = "/very/long/path/".repeat(100);
    let uris = [
        "/",
        "/index.php",
        "/api/v1/users",
        long_uri.as_str(),
        "/path?query=value",
        "/path#fragment",
    ];

    for uri in uris {
        let mut worker = Worker::spawn(0, PathBuf::from("/tmp/test"), 512)
            .await
            .unwrap();

        let result = worker
            .handle_request("GET".into(), uri.to_string(), 5000)
            .await;

        assert!(result.is_err(), "uri {} should fail on stub", uri);
    }
}
