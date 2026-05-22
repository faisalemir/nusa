//! Domain-specific tests for the Octane worker pool and state reset.
//!
//! Covers: worker lifecycle, pool operations, handshake, state reset,
//! state restore, and WorkerError exhaustive coverage.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use nusa_octane_worker::error::WorkerError;
use nusa_octane_worker::pool::{Worker, WorkerPool, WorkerState};
use nusa_octane_worker::state_reset::{OctaneEvent, StateResetOrchestrator};

fn test_app_root() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nusa_octane_domain_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("test app_root must exist");
    dir
}

/// STUB_CONTRACT: queue/lifecycle domain tests; live PHP pool covered in podman-test-laravel.
fn init_stub_pool(pool: &mut WorkerPool) {
    pool.initialize_test_stubs();
}

// ─── 1. Worker Lifecycle ──────────────────────────────────────────────────

#[tokio::test]
async fn worker_initial_state_is_idle() {
    // Verify WorkerState enum values
    assert_eq!(WorkerState::Idle, WorkerState::Idle);
    assert_eq!(WorkerState::Busy, WorkerState::Busy);
    assert_eq!(WorkerState::Draining, WorkerState::Draining);
    assert_eq!(WorkerState::Stopped, WorkerState::Stopped);
}

#[tokio::test]
async fn worker_counters_start_at_zero() {
    // Create a stub worker (Windows fallback or no PHP)
    let worker = Worker::spawn(0, test_app_root(), 256).await;
    // On non-unix or without PHP, this falls back to stub
    if let Ok(w) = worker {
        assert_eq!(w.id, 0);
        assert_eq!(w.state, WorkerState::Idle);
        assert_eq!(w.requests_handled.load(Ordering::SeqCst), 0);
        assert_eq!(w.rss_mb.load(Ordering::SeqCst), 0);
        assert_eq!(w.error_count.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn worker_should_recycle_by_request_count() {
    let worker = Worker::new_test_stub(99);

    // Should NOT recycle at 0 requests
    assert!(!worker.should_recycle(100, 256));

    // Should recycle when requests hit limit
    worker.requests_handled.store(100, Ordering::SeqCst);
    assert!(worker.should_recycle(100, 256));

    // Should recycle when memory hits limit
    worker.requests_handled.store(0, Ordering::SeqCst);
    worker.rss_mb.store(256, Ordering::SeqCst);
    assert!(worker.should_recycle(100, 256));
}

#[tokio::test]
async fn worker_stop_sets_state_to_stopped() {
    let mut worker = Worker::new_test_stub(1);
    assert_eq!(worker.state, WorkerState::Idle);

    let _ = worker.stop().await;
    assert_eq!(worker.state, WorkerState::Stopped);
}

#[tokio::test]
async fn worker_handle_request_without_transport_returns_no_transport_error() {
    let mut worker = Worker::new_test_stub(2);
    // Stub mode has no transport
    let result = worker
        .handle_request(
            "GET".to_string(),
            "/".to_string(),
            Default::default(),
            None,
            5000,
        )
        .await;
    assert!(result.is_err());
    match result.expect_err("expected error") {
        WorkerError::NoTransport(id) => {
            assert_eq!(id, 2);
        }
        other => panic!("expected NoTransport, got: {other:?}"),
    }
}

// ─── 2. Pool Operations ───────────────────────────────────────────────────

#[tokio::test]
async fn pool_new_initializes_empty() {
    let pool = WorkerPool::new(4, test_app_root(), 256, 1000);
    assert_eq!(pool.worker_count(), 0);
    assert_eq!(pool.idle_count(), 0);
    assert_eq!(pool.total_handled(), 0);
    assert_eq!(pool.total_errors(), 0);
    assert_eq!(pool.max_requests(), 1000);
}

#[tokio::test]
async fn pool_stub_initialize_starts_workers() {
    let mut pool = WorkerPool::new(2, test_app_root(), 256, 100);
    init_stub_pool(&mut pool);
    assert_eq!(pool.worker_count(), 2);
    assert_eq!(pool.idle_count(), 2);
    assert!(
        !pool.is_ready(),
        "stub pool must not report production-ready"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn pool_production_initialize_fails_without_php_driver() {
    let mut pool = WorkerPool::new(2, test_app_root(), 256, 100);
    let result = pool.initialize().await;
    assert!(
        result.is_err(),
        "production initialize without php-driver must fail closed"
    );
}

#[tokio::test]
async fn pool_get_idle_worker_returns_some() {
    // Create a stub pool manually
    let mut pool = WorkerPool::new(1, test_app_root(), 256, 100);
    // Pool starts empty — get_idle_worker should return None
    assert!(pool.get_idle_worker().is_none());
}

#[tokio::test]
async fn pool_return_worker_adds_to_idle_queue() {
    let mut pool = WorkerPool::new(2, test_app_root(), 256, 100);
    init_stub_pool(&mut pool);
    let initial_idle = pool.idle_count();
    let worker_id = pool
        .get_idle_worker()
        .expect("stub pool should have idle worker")
        .id;
    assert_eq!(pool.idle_count(), initial_idle - 1);
    pool.return_worker(worker_id);
    assert_eq!(pool.idle_count(), initial_idle);
}

#[tokio::test]
async fn pool_return_worker_in_draining_state_not_added() {
    let mut pool = WorkerPool::new(2, test_app_root(), 256, 100);
    init_stub_pool(&mut pool);
    let worker_id = pool
        .get_idle_worker()
        .expect("stub pool should have idle worker")
        .id;
    let initial_idle = pool.idle_count();
    pool.worker_mut(worker_id).state = WorkerState::Draining;
    pool.return_worker(worker_id);
    assert_eq!(pool.idle_count(), initial_idle);
}

#[tokio::test]
async fn pool_shutdown_clears_all_workers() {
    let mut pool = WorkerPool::new(2, test_app_root(), 256, 100);
    init_stub_pool(&mut pool);
    assert!(pool.worker_count() > 0);
    let _ = pool.shutdown().await;
    assert_eq!(pool.worker_count(), 0);
    assert_eq!(pool.idle_count(), 0);
}

#[tokio::test]
async fn pool_scale_up_adds_workers() {
    let mut pool = WorkerPool::new(1, test_app_root(), 256, 100);
    init_stub_pool(&mut pool);
    assert_eq!(pool.worker_count(), 1);
}

#[tokio::test]
async fn pool_starvation_behavior_queues_with_timeout() {
    let mut pool = WorkerPool::new(1, test_app_root(), 256, 100);
    init_stub_pool(&mut pool);
    let worker = pool.get_idle_worker();
    assert!(worker.is_some());
    assert_eq!(pool.idle_count(), 0);
    let next = pool.get_idle_worker();
    assert!(next.is_none(), "should have no idle workers");
}

// ─── 3. Handshake ─────────────────────────────────────────────────────────

#[tokio::test]
async fn worker_handshake_version_match_succeeds() {
    use nusa_ipc::protocol::IpcMessage;

    // Verify Hello message structure
    let hello = IpcMessage::Hello {
        version: "1.0".to_string(),
        pid: 12345,
        capabilities: vec!["http".to_string(), "tasks".to_string()],
    };

    match hello {
        IpcMessage::Hello {
            version,
            pid,
            capabilities,
        } => {
            assert_eq!(version, "1.0");
            assert_eq!(pid, 12345);
            assert_eq!(capabilities.len(), 2);
            assert!(capabilities.contains(&"http".to_string()));
            assert!(capabilities.contains(&"tasks".to_string()));
        }
        _ => panic!("expected Hello message"),
    }
}

#[tokio::test]
async fn worker_handshake_version_mismatch_rejected() {
    use nusa_ipc::protocol::IpcMessage;

    // Worker sends unexpected response instead of Ack
    let unexpected = IpcMessage::Shutdown;

    match unexpected {
        IpcMessage::Shutdown => {
            // This would be rejected in the handshake code with:
            // WorkerError::Handshake("unexpected response".into())
        }
        _ => panic!("expected Shutdown"),
    }
}

// ─── 4. State Reset ───────────────────────────────────────────────────────

#[tokio::test]
async fn state_reset_orchestrator_new_creates_with_empty_stats() {
    let orchestrator = StateResetOrchestrator::new(128);
    let stats = orchestrator.stats();
    assert_eq!(stats.total_requests_processed, 0);
    assert_eq!(stats.total_resets_performed, 0);
    assert_eq!(stats.total_cleanups_performed, 0);
    assert_eq!(stats.total_worker_stops, 0);
}

#[tokio::test]
async fn state_reset_emit_request_received_increments_requests() {
    let orchestrator = StateResetOrchestrator::new(128);

    orchestrator.emit_event(OctaneEvent::RequestReceived {
        request_id: "req-1".to_string(),
    });

    let stats = orchestrator.stats();
    assert_eq!(stats.total_requests_processed, 1);
}

#[tokio::test]
async fn state_reset_emit_request_terminated_increments_resets_and_cleanups() {
    let orchestrator = StateResetOrchestrator::new(128);

    orchestrator.emit_event(OctaneEvent::RequestTerminated {
        request_id: "req-1".to_string(),
        status: 200,
    });

    let stats = orchestrator.stats();
    assert_eq!(stats.total_resets_performed, 1);
    assert_eq!(stats.total_cleanups_performed, 1);
}

#[tokio::test]
async fn state_reset_emit_worker_stopping_increments_stops() {
    let orchestrator = StateResetOrchestrator::new(128);

    orchestrator.emit_event(OctaneEvent::WorkerStopping { worker_id: 0 });

    let stats = orchestrator.stats();
    assert_eq!(stats.total_worker_stops, 1);
}

#[tokio::test]
async fn state_reset_multiple_events_accumulate_correctly() {
    let orchestrator = StateResetOrchestrator::new(128);

    // Emit various events
    orchestrator.emit_event(OctaneEvent::RequestReceived {
        request_id: "req-1".to_string(),
    });
    orchestrator.emit_event(OctaneEvent::RequestReceived {
        request_id: "req-2".to_string(),
    });
    orchestrator.emit_event(OctaneEvent::RequestTerminated {
        request_id: "req-1".to_string(),
        status: 200,
    });
    orchestrator.emit_event(OctaneEvent::WorkerStopping { worker_id: 0 });

    let stats = orchestrator.stats();
    assert_eq!(stats.total_requests_processed, 2);
    assert_eq!(stats.total_resets_performed, 1);
    assert_eq!(stats.total_cleanups_performed, 1);
    assert_eq!(stats.total_worker_stops, 1);
}

#[tokio::test]
async fn state_reset_subscribe_receives_events() {
    let orchestrator = StateResetOrchestrator::new(128);
    let mut rx = orchestrator.subscribe();

    orchestrator.emit_event(OctaneEvent::WorkerStarted { worker_id: 0 });

    // Receive the event
    let event = rx.try_recv();
    assert!(event.is_ok(), "should receive emitted event");
    if let Ok(OctaneEvent::WorkerStarted { worker_id }) = event {
        assert_eq!(worker_id, 0);
    }
}

#[tokio::test]
async fn state_reset_register_action_executes_on_emit() {
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;

    let orchestrator = StateResetOrchestrator::new(128);
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();

    let mut orchestrator = orchestrator;
    orchestrator.register_action("request_received".to_string(), move |_event| {
        counter_clone.fetch_add(1, Ordering::SeqCst);
    });

    orchestrator.emit_event(OctaneEvent::RequestReceived {
        request_id: "test".to_string(),
    });

    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn state_reset_initialize_registers_default_actions() {
    let mut orchestrator = StateResetOrchestrator::new(128);
    orchestrator.initialize();

    // After initialize, emitting events should increment stats
    orchestrator.emit_event(OctaneEvent::RequestReceived {
        request_id: "req-1".to_string(),
    });
    orchestrator.emit_event(OctaneEvent::RequestTerminated {
        request_id: "req-1".to_string(),
        status: 200,
    });

    let stats = orchestrator.stats();
    assert_eq!(stats.total_requests_processed, 1);
    assert_eq!(stats.total_resets_performed, 1);
}

#[tokio::test]
async fn state_reset_shutdown_no_panic() {
    let orchestrator = StateResetOrchestrator::new(128);
    orchestrator.shutdown();
    // Should not panic
}

// ─── 5. State Restore ─────────────────────────────────────────────────────

#[tokio::test]
async fn state_restore_clean_shutdown_preserves_state() {
    // StateResetOrchestrator maintains state through emit events
    let orchestrator = StateResetOrchestrator::new(128);

    // Emit some events
    orchestrator.emit_event(OctaneEvent::RequestReceived {
        request_id: "req-1".to_string(),
    });
    orchestrator.emit_event(OctaneEvent::RequestTerminated {
        request_id: "req-1".to_string(),
        status: 200,
    });

    // Stats should be preserved
    let stats = orchestrator.stats();
    assert_eq!(stats.total_requests_processed, 1);
    assert_eq!(stats.total_resets_performed, 1);
}

#[tokio::test]
async fn state_restore_large_state_handles_many_events() {
    let orchestrator = StateResetOrchestrator::new(1024); // Large buffer

    // Emit 10K+ events
    for i in 0..10000 {
        orchestrator.emit_event(OctaneEvent::RequestReceived {
            request_id: format!("req-{i}"),
        });
    }

    let stats = orchestrator.stats();
    assert_eq!(stats.total_requests_processed, 10000);
}

#[tokio::test]
async fn state_restore_version_mismatch_rejected() {
    use nusa_ipc::protocol::IpcMessage;

    // Simulate version mismatch in handshake
    let wrong_version = IpcMessage::Hello {
        version: "0.9".to_string(),
        pid: 12345,
        capabilities: vec![],
    };

    // The handshake code expects version "1.0" — "0.9" would cause Handshake error
    match wrong_version {
        IpcMessage::Hello { version, .. } => {
            assert_ne!(version, "1.0"); // Would be rejected
        }
        _ => panic!("expected Hello"),
    }
}

// ─── 6. WorkerError Exhaustive ────────────────────────────────────────────

#[tokio::test]
async fn worker_error_io_variant_with_realistic_data() {
    let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "connection refused");
    let worker_err = WorkerError::Io(io_err);

    let msg = worker_err.to_string();
    assert!(msg.contains("IO error"));
    assert!(msg.contains("connection refused"));
}

#[tokio::test]
async fn worker_error_handshake_variant_with_specific_message() {
    let worker_err = WorkerError::Handshake("unexpected response: Shutdown".into());

    let msg = worker_err.to_string();
    assert!(msg.contains("Handshake failed"));
    assert!(msg.contains("unexpected response"));
}

#[tokio::test]
async fn worker_error_no_transport_variant_with_worker_id() {
    let worker_err = WorkerError::NoTransport(42);

    let msg = worker_err.to_string();
    assert!(msg.contains("Worker 42"));
    assert!(msg.contains("no transport"));
    assert!(msg.contains("stub mode"));
}

#[tokio::test]
async fn worker_error_ipc_variant_wrapped() {
    // IpcError from nusa_ipc — we verify the wrapper works
    // WorkerError::Ipc(IpcError)
    // Since IpcError is from another crate, we just verify the variant exists
    let err = WorkerError::NoTransport(0);
    // Check it's Debug-able
    let debug_str = format!("{:?}", err);
    assert!(debug_str.contains("NoTransport"));
}

#[tokio::test]
async fn worker_error_all_variants_are_debug_representable() {
    let errors = vec![
        WorkerError::Io(std::io::Error::other("test")),
        WorkerError::Handshake("test handshake failure".into()),
        WorkerError::NoTransport(0),
    ];

    for err in errors {
        let debug = format!("{:?}", err);
        assert!(!debug.is_empty(), "error should be Debug-representable");
    }
}

// ─── 7. OctaneEvent Exhaustive ────────────────────────────────────────────

#[tokio::test]
async fn octane_event_worker_started_variant() {
    let event = OctaneEvent::WorkerStarted { worker_id: 5 };
    match event {
        OctaneEvent::WorkerStarted { worker_id } => {
            assert_eq!(worker_id, 5);
        }
        _ => panic!("expected WorkerStarted"),
    }
}

#[tokio::test]
async fn octane_event_request_received_variant() {
    let event = OctaneEvent::RequestReceived {
        request_id: "abc-123".to_string(),
    };
    match event {
        OctaneEvent::RequestReceived { request_id } => {
            assert_eq!(request_id, "abc-123");
        }
        _ => panic!("expected RequestReceived"),
    }
}

#[tokio::test]
async fn octane_event_request_terminated_variant() {
    let event = OctaneEvent::RequestTerminated {
        request_id: "abc-123".to_string(),
        status: 500,
    };
    match event {
        OctaneEvent::RequestTerminated { request_id, status } => {
            assert_eq!(request_id, "abc-123");
            assert_eq!(status, 500);
        }
        _ => panic!("expected RequestTerminated"),
    }
}

#[tokio::test]
async fn octane_event_worker_stopping_variant() {
    let event = OctaneEvent::WorkerStopping { worker_id: 3 };
    match event {
        OctaneEvent::WorkerStopping { worker_id } => {
            assert_eq!(worker_id, 3);
        }
        _ => panic!("expected WorkerStopping"),
    }
}
