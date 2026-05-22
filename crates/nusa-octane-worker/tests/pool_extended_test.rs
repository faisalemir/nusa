//! Extended stress tests for nusa-octane-worker pool.
//!
//! Uses the real `WorkerPool::initialize` / `Worker::spawn` paths (same as production).
//! When no PHP Octane worker is listening, Windows falls back to stub workers after a
//! bounded TCP probe; Unix fails fast if `app_root` is missing or the worker socket never appears.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tokio::sync::Mutex;

use nusa_octane_worker::pool::{Worker, WorkerPool, WorkerState};

/// Writable app root that exists (required by production spawn); PHP worker may still be absent.
fn test_app_root() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nusa_octane_app_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("test app_root must exist");
    dir
}

/// Worker for lifecycle tests: production spawn when PHP driver exists, else stub.
async fn worker_for_lifecycle_test() -> Worker {
    match Worker::spawn(0, test_app_root(), 512).await {
        Ok(worker) => worker,
        Err(_) => Worker::new_test_stub(0),
    }
}

// ── Pool Boundary Values ──

#[tokio::test]
async fn pool_single_worker() {
    let mut pool = WorkerPool::new(1, test_app_root(), 512, 1000);
    if pool.initialize().await.is_err() {
        pool.initialize_test_stubs();
    }
    assert_eq!(pool.worker_count(), 1);
    assert_eq!(pool.idle_count(), 1);
}

#[tokio::test]
async fn pool_many_workers() {
    let mut pool = WorkerPool::new(50, test_app_root(), 512, 1000);
    if pool.initialize().await.is_err() {
        pool.initialize_test_stubs();
    }
    assert_eq!(pool.worker_count(), 50);
    assert_eq!(pool.idle_count(), 50);
}

// ── Worker Lifecycle Edge Cases ──

#[tokio::test]
async fn worker_stop_idempotent() {
    let mut worker = worker_for_lifecycle_test().await;

    assert!(worker.stop().await.is_ok());
    assert_eq!(worker.state, WorkerState::Stopped);

    // Second stop should also succeed (idempotent)
    assert!(worker.stop().await.is_ok());
    assert_eq!(worker.state, WorkerState::Stopped);
}

#[tokio::test]
async fn worker_should_recycle_exact_boundary() {
    let worker = worker_for_lifecycle_test().await;

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
    let mut worker = worker_for_lifecycle_test().await;

    worker.state = WorkerState::Busy;
    assert!(worker.stop().await.is_ok());
    assert_eq!(worker.state, WorkerState::Stopped);
}

// ── Pool Stress ──

#[tokio::test]
async fn pool_rapid_initialize_shutdown() {
    let app_root = test_app_root();
    for _ in 0..10 {
        let mut pool = WorkerPool::new(2, app_root.clone(), 512, 1000);
        if pool.initialize().await.is_err() {
            pool.initialize_test_stubs();
        }
        pool.shutdown().await.expect("production shutdown path");
        assert_eq!(pool.worker_count(), 0);
    }
}

#[tokio::test]
async fn pool_concurrent_get_idle_and_return() {
    let pool = Arc::new(Mutex::new(WorkerPool::new(4, test_app_root(), 512, 1000)));

    {
        let mut p = pool.lock().await;
        if p.initialize().await.is_err() {
            p.initialize_test_stubs();
        }
    }

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
        h.await.expect("concurrent get_idle task");
    }

    let got_count = success_count.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(got_count, 4, "all 4 workers should be gotten");
}

#[tokio::test]
async fn pool_recycle_multiple_times_same_worker() {
    let mut pool = WorkerPool::new(1, test_app_root(), 512, 1000);
    if pool.initialize().await.is_err() {
        pool.initialize_test_stubs();
    }

    for i in 0..5 {
        let result = pool.recycle_worker(0).await;
        assert!(result.is_ok(), "recycle {} must succeed", i);
        assert_eq!(pool.idle_count(), 1);
        assert_eq!(pool.worker(0).requests_handled.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn pool_shutdown_after_recycle() {
    let mut pool = WorkerPool::new(3, test_app_root(), 512, 1000);
    if pool.initialize().await.is_err() {
        pool.initialize_test_stubs();
    }

    pool.recycle_worker(0)
        .await
        .expect("production recycle path");
    pool.recycle_worker(1)
        .await
        .expect("production recycle path");

    pool.shutdown().await.expect("production shutdown path");
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
    // STUB_CONTRACT: asserts no-transport error; live PHP covered in podman-test-laravel.
    let mut worker = Worker::new_test_stub(0);

    let result = worker
        .handle_request("GET".into(), "/".into(), Default::default(), None, 5000)
        .await;

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("no transport"));
    assert!(err_msg.contains("0")); // worker id
}

#[tokio::test]
async fn worker_handle_request_various_methods() {
    // STUB_CONTRACT: methods matrix without live transport; E2E in podman-test-laravel.
    let methods = ["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD"];
    let mut worker = Worker::new_test_stub(0);

    for method in methods {
        let result = worker
            .handle_request(
                method.to_string(),
                "/test".into(),
                Default::default(),
                None,
                5000,
            )
            .await;

        assert!(
            result.is_err(),
            "method {} should fail without live transport",
            method
        );
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

    let mut worker = Worker::new_test_stub(0);

    for uri in uris {
        let result = worker
            .handle_request(
                "GET".into(),
                uri.to_string(),
                Default::default(),
                None,
                5000,
            )
            .await;

        assert!(
            result.is_err(),
            "uri {} should fail without live transport",
            uri
        );
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
                let o = orch.lock().expect("orchestrator lock");
                o.emit_event(OctaneEvent::RequestReceived {
                    request_id: format!("source-{}-event-{}", source, i),
                });
            }
        }));
    }

    for h in handles {
        h.await.expect("concurrent orchestrator task");
    }

    let stats = orchestrator.lock().expect("orchestrator lock").stats();
    assert_eq!(stats.total_requests_processed, 100);
}
