//! Extended stress tests for nusa-octane-worker pool.
//!
//! Covers: pool boundary values, concurrent operations, worker lifecycle edge cases.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

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
    let pool = Arc::new(Mutex::new(WorkerPool::new(
        4,
        PathBuf::from("/tmp/test"),
        512,
        1000,
    )));

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

    // Register actions with different names (HashMap key = event name)
    orchestrator.register_action("request_received".to_string(), {
        let c = counter.clone();
        move |_| {
            c.fetch_add(1, Ordering::SeqCst);
        }
    });
    orchestrator.register_action("request_terminated".to_string(), {
        let c = counter.clone();
        move |_| {
            c.fetch_add(1, Ordering::SeqCst);
        }
    });
    orchestrator.register_action("worker_stopping".to_string(), {
        let c = counter.clone();
        move |_| {
            c.fetch_add(1, Ordering::SeqCst);
        }
    });

    // Emit all three event types
    orchestrator.emit_event(OctaneEvent::RequestReceived {
        request_id: "req-1".to_string(),
    });
    orchestrator.emit_event(OctaneEvent::RequestTerminated {
        request_id: "req-1".to_string(),
        status: 200,
    });
    orchestrator.emit_event(OctaneEvent::WorkerStopping { worker_id: 0 });

    // All three actions should execute (one per event type)
    assert_eq!(counter.load(Ordering::SeqCst), 3);
}

// ── Worker handle_request with Various Combos ──

#[tokio::test]
async fn worker_handle_request_stub_error_message() {
    let mut worker = Worker::spawn(0, PathBuf::from("/tmp/test"), 512)
        .await
        .unwrap();

    let result = worker.handle_request("GET".into(), "/".into(), 5000).await;

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

// ── Event-Driven Logic Extended ──

/// Event replay → same events produce same result.
#[tokio::test]
async fn orchestrator_event_replay_produces_same_result() {
    use nusa_octane_worker::state_reset::{OctaneEvent, StateResetOrchestrator};

    // First replay
    let mut orchestrator1 = StateResetOrchestrator::new(128);
    orchestrator1.initialize();
    orchestrator1.emit_event(OctaneEvent::WorkerStarted { worker_id: 1 });
    orchestrator1.emit_event(OctaneEvent::RequestReceived {
        request_id: "r1".into(),
    });
    orchestrator1.emit_event(OctaneEvent::RequestTerminated {
        request_id: "r1".into(),
        status: 200,
    });
    let stats1 = orchestrator1.stats();

    // Second replay with same events
    let mut orchestrator2 = StateResetOrchestrator::new(128);
    orchestrator2.initialize();
    orchestrator2.emit_event(OctaneEvent::WorkerStarted { worker_id: 1 });
    orchestrator2.emit_event(OctaneEvent::RequestReceived {
        request_id: "r1".into(),
    });
    orchestrator2.emit_event(OctaneEvent::RequestTerminated {
        request_id: "r1".into(),
        status: 200,
    });
    let stats2 = orchestrator2.stats();

    // Same events should produce same stats
    assert_eq!(
        stats1.total_requests_processed,
        stats2.total_requests_processed
    );
    assert_eq!(stats1.total_resets_performed, stats2.total_resets_performed);
    assert_eq!(
        stats1.total_cleanups_performed,
        stats2.total_cleanups_performed
    );
}

/// Event storm → N events per second, no loss.
#[tokio::test]
async fn orchestrator_event_storm_no_loss() {
    use nusa_octane_worker::state_reset::{OctaneEvent, StateResetOrchestrator};
    use std::sync::atomic::{AtomicU64, Ordering};

    let received = Arc::new(AtomicU64::new(0));
    let mut orchestrator = StateResetOrchestrator::new(10_000);

    let recv_counter = received.clone();
    orchestrator.register_action("request_received".to_string(), move |_| {
        recv_counter.fetch_add(1, Ordering::SeqCst);
    });

    // Fire 1000 events rapidly
    for i in 0..1000 {
        orchestrator.emit_event(OctaneEvent::RequestReceived {
            request_id: format!("storm-{}", i),
        });
    }

    let count = received.load(Ordering::SeqCst);
    assert_eq!(count, 1000, "all 1000 storm events should be processed");
}

/// Event after timeout → stale event handling.
#[tokio::test]
async fn orchestrator_stale_event_handling() {
    use nusa_octane_worker::state_reset::{OctaneEvent, StateResetOrchestrator};

    let mut orchestrator = StateResetOrchestrator::new(128);
    orchestrator.initialize();

    // Emit event
    orchestrator.emit_event(OctaneEvent::RequestReceived {
        request_id: "stale-event".into(),
    });

    // Wait (simulating timeout)
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Emit another event — should still work normally
    orchestrator.emit_event(OctaneEvent::RequestReceived {
        request_id: "fresh-event".into(),
    });

    let stats = orchestrator.stats();
    assert_eq!(stats.total_requests_processed, 2);
}

/// Concurrent events from multiple sources.
#[tokio::test]
async fn orchestrator_concurrent_events_from_multiple_sources() {
    use nusa_octane_worker::state_reset::{OctaneEvent, StateResetOrchestrator};
    use std::sync::Arc;

    let orchestrator = Arc::new(std::sync::Mutex::new({
        let mut o = StateResetOrchestrator::new(10_000);
        o.initialize();
        o
    }));

    let mut handles = Vec::new();
    for source in 0..5 {
        let orch = orchestrator.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..20 {
                let o = orch.lock().unwrap();
                o.emit_event(OctaneEvent::RequestReceived {
                    request_id: format!("source-{}-event-{}", source, i),
                });
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let stats = orchestrator.lock().unwrap().stats();
    assert_eq!(stats.total_requests_processed, 100);
}
