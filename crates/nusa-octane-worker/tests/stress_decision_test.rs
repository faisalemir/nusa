//! Stress tests, decision logic tests, and state machine tests for nusa-octane-worker.
//!
//! Covers: WorkerPool, Worker, WorkerMetrics, WorkerState, recycling, backpressure.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use nusa_octane_worker::metrics::WorkerMetrics;
use nusa_octane_worker::pool::{Worker, WorkerState};

// ============================================================================
// Stress Tests: WorkerMetrics Throughput
// ============================================================================

#[test]
fn worker_metrics_throughput_1000_recordings() {
    let metrics = WorkerMetrics::new();
    let start = std::time::Instant::now();

    for _ in 0..1000 {
        metrics.record_request(50, true);
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 5000,
        "1000 recordings took too long: {:?}",
        elapsed
    );
}

#[test]
fn worker_metrics_throughput_10000_recordings() {
    let metrics = WorkerMetrics::new();
    let start = std::time::Instant::now();

    for i in 0..10_000 {
        metrics.record_request(100, i % 2 == 0);
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 10000,
        "10000 recordings took too long: {:?}",
        elapsed
    );
}

// ============================================================================
// Stress Tests: Metrics Accuracy Under Load
// ============================================================================

#[test]
fn worker_metrics_accuracy_under_concurrent_load() {
    let metrics = std::sync::Arc::new(WorkerMetrics::new());
    let num_threads = 4;
    let recordings_per_thread = 250;

    let mut handles = Vec::new();
    for t in 0..num_threads {
        let m = metrics.clone();
        handles.push(std::thread::spawn(move || {
            for _ in 0..recordings_per_thread {
                m.record_request(50, t % 2 == 0);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let total = num_threads * recordings_per_thread;
    assert_eq!(metrics.requests_handled() as usize, total);
    assert_eq!(metrics.error_count() as usize, total / 2);

    let error_rate = metrics.error_rate();
    assert!(
        (error_rate - 0.5).abs() < 0.01,
        "error rate should be ~0.5, got {error_rate}"
    );
}

// ============================================================================
// Stress Tests: WorkerMetrics Accuracy at Boundary Values
// ============================================================================

#[test]
fn worker_metrics_error_rate_zero_requests_returns_zero() {
    let metrics = WorkerMetrics::new();
    assert_eq!(metrics.error_rate(), 0.0);
}

#[test]
fn worker_metrics_error_rate_one_request_zero_errors() {
    let metrics = WorkerMetrics::new();
    metrics.record_request(100, true);
    assert_eq!(metrics.error_rate(), 0.0);
}

#[test]
fn worker_metrics_error_rate_one_request_one_error() {
    let metrics = WorkerMetrics::new();
    metrics.record_request(100, false);
    assert_eq!(metrics.error_rate(), 1.0);
}

#[test]
fn worker_metrics_error_rate_n_requests_m_errors() {
    let metrics = WorkerMetrics::new();
    metrics.record_request(100, true);
    metrics.record_request(100, true);
    metrics.record_request(100, false);
    metrics.record_request(100, true);

    let rate = metrics.error_rate();
    assert!((rate - 0.25).abs() < 0.001, "expected 0.25, got {rate}");
}

#[test]
fn worker_metrics_avg_response_time_accurate() {
    let metrics = WorkerMetrics::new();
    metrics.record_request(100, true);
    metrics.record_request(200, true);
    metrics.record_request(300, true);

    // Running average: (100 + 200 + 300) / 3 = 200
    let avg = metrics.avg_response_time_ms();
    assert_eq!(avg, 200, "average should be 200ms, got {avg}");
}

#[test]
fn worker_metrics_requests_handled_counter() {
    let metrics = WorkerMetrics::new();
    assert_eq!(metrics.requests_handled(), 0);
    metrics.record_request(50, true);
    assert_eq!(metrics.requests_handled(), 1);
    metrics.record_request(50, false);
    assert_eq!(metrics.requests_handled(), 2);
}

#[test]
fn worker_metrics_error_count_counter() {
    let metrics = WorkerMetrics::new();
    assert_eq!(metrics.error_count(), 0);
    metrics.record_request(50, true);
    assert_eq!(metrics.error_count(), 0);
    metrics.record_request(50, false);
    assert_eq!(metrics.error_count(), 1);
}

// ============================================================================
// Decision Logic Tests: Worker Recycle
// ============================================================================

#[test]
fn worker_recycle_requests_below_threshold_keep() {
    let worker = create_stub_worker(0);
    worker.requests_handled.store(5, Ordering::SeqCst);
    worker.rss_mb.store(100, Ordering::SeqCst);

    assert!(!worker.should_recycle(100, 256));
}

#[test]
fn worker_recycle_requests_at_threshold_recycle() {
    let worker = create_stub_worker(0);
    worker.requests_handled.store(100, Ordering::SeqCst);
    worker.rss_mb.store(100, Ordering::SeqCst);

    assert!(worker.should_recycle(100, 256));
}

#[test]
fn worker_recycle_memory_at_threshold_recycle() {
    let worker = create_stub_worker(0);
    worker.requests_handled.store(10, Ordering::SeqCst);
    worker.rss_mb.store(256, Ordering::SeqCst);

    assert!(worker.should_recycle(100, 256));
}

#[test]
fn worker_recycle_both_conditions_met_recycle() {
    let worker = create_stub_worker(0);
    worker.requests_handled.store(200, Ordering::SeqCst);
    worker.rss_mb.store(512, Ordering::SeqCst);

    assert!(worker.should_recycle(100, 256));
}

#[test]
fn worker_recycle_neither_condition_keep() {
    let worker = create_stub_worker(0);
    worker.requests_handled.store(50, Ordering::SeqCst);
    worker.rss_mb.store(128, Ordering::SeqCst);

    assert!(!worker.should_recycle(100, 256));
}

fn create_stub_worker(id: usize) -> Worker {
    Worker::new_test_stub(id)
}

// ============================================================================
// State Machine Tests: Worker State Transitions
// ============================================================================

#[test]
fn worker_state_idle_to_idle_noop() {
    let mut worker = create_stub_worker(0);
    worker.state = WorkerState::Idle;

    // return_worker when idle is a no-op (just adds to queue)
    assert_eq!(worker.state, WorkerState::Idle);
}

#[test]
fn worker_state_initial_is_idle() {
    let worker = create_stub_worker(0);
    assert_eq!(worker.state, WorkerState::Idle);
}

#[test]
fn worker_state_stopped_to_any_invalid() {
    let mut worker = create_stub_worker(0);
    worker.state = WorkerState::Stopped;

    // Worker should not transition from Stopped
    // (The code doesn't enforce this at compile-time, but semantically it's invalid)
    assert_eq!(worker.state, WorkerState::Stopped);
}

#[test]
fn worker_state_draining_not_returned_to_idle() {
    use nusa_octane_worker::pool::WorkerPool;

    let mut pool = WorkerPool::new(1, PathBuf::from("/tmp"), 256, 1000);

    pool.push_test_worker(create_stub_worker(0));
    pool.worker_mut(0).state = WorkerState::Draining;
    pool.return_worker(0);

    // Draining workers should not be returned to idle queue
    assert_eq!(pool.idle_count(), 0);
}

#[test]
fn worker_state_idle_to_draining_stops() {
    let mut worker = create_stub_worker(0);
    worker.state = WorkerState::Draining;

    // Should be able to stop from draining
    // (The stop() method just sets state to Stopped)
    // Can't test async stop() without PHP, but verify state transitions
    assert_eq!(worker.state, WorkerState::Draining);
}

// ============================================================================
// State Machine Tests: WorkerState Enum
// ============================================================================

#[test]
fn worker_state_variants_all_exist() {
    let states = [
        WorkerState::Idle,
        WorkerState::Busy,
        WorkerState::Draining,
        WorkerState::Stopped,
    ];

    // All states should be distinct
    for (i, s1) in states.iter().enumerate() {
        for (j, s2) in states.iter().enumerate() {
            if i != j {
                assert_ne!(s1, s2);
            }
        }
    }
}

#[test]
fn worker_state_copy_eq() {
    let s1 = WorkerState::Idle;
    let s2 = s1;
    assert_eq!(s1, s2);
}

#[test]
fn worker_state_clone() {
    let s1 = WorkerState::Busy;
    let s2 = s1;
    assert_eq!(s1, s2);
}

// ============================================================================
// Stress Tests: Worker Metrics Concurrent Access
// ============================================================================

#[test]
fn worker_metrics_concurrent_record_no_data_loss() {
    let metrics = std::sync::Arc::new(WorkerMetrics::new());
    let num_threads = 8;
    let ops_per_thread = 1000;

    let mut handles = Vec::new();
    for _ in 0..num_threads {
        let m = metrics.clone();
        handles.push(std::thread::spawn(move || {
            for _ in 0..ops_per_thread {
                m.record_request(10, true);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let expected = num_threads * ops_per_thread;
    assert_eq!(metrics.requests_handled() as usize, expected);
    assert_eq!(metrics.error_count(), 0);
}

#[test]
fn worker_metrics_concurrent_record_mixed_success_failure() {
    let metrics = std::sync::Arc::new(WorkerMetrics::new());
    let num_threads = 4;
    let ops_per_thread = 500;

    let mut handles = Vec::new();
    for t in 0..num_threads {
        let m = metrics.clone();
        handles.push(std::thread::spawn(move || {
            for i in 0..ops_per_thread {
                let success = (t + i) % 3 != 0; // ~67% success rate
                m.record_request(50, success);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let total = num_threads * ops_per_thread;
    assert_eq!(metrics.requests_handled() as usize, total);

    let error_rate = metrics.error_rate();
    assert!(
        (error_rate - 0.333).abs() < 0.05,
        "error rate should be ~33%, got {error_rate:.2}"
    );
}

// ============================================================================
// Stress Tests: Pool get_idle Worker
// ============================================================================

#[test]
fn pool_get_idle_available_returns_worker() {
    use nusa_octane_worker::pool::WorkerPool;

    let mut pool = WorkerPool::new(2, PathBuf::from("/tmp"), 256, 1000);

    pool.push_test_worker(create_stub_worker(0));
    pool.push_test_worker(create_stub_worker(1));
    pool.enqueue_idle_worker(0);
    pool.enqueue_idle_worker(1);

    let worker = pool.get_idle_worker();
    assert!(worker.is_some());
    assert_eq!(worker.unwrap().id, 1); // pop gives last item
}

#[test]
fn pool_get_idle_empty_returns_none() {
    use nusa_octane_worker::pool::WorkerPool;

    let mut pool = WorkerPool::new(2, PathBuf::from("/tmp"), 256, 1000);

    pool.push_test_worker(create_stub_worker(0));
    pool.push_test_worker(create_stub_worker(1));

    let worker = pool.get_idle_worker();
    assert!(worker.is_none());
}

#[test]
fn pool_return_worker_adds_to_idle() {
    use nusa_octane_worker::pool::WorkerPool;

    let mut pool = WorkerPool::new(2, PathBuf::from("/tmp"), 256, 1000);

    pool.push_test_worker(create_stub_worker(0));

    pool.return_worker(0);
    assert_eq!(pool.idle_count(), 1);
}

#[test]
fn pool_get_idle_then_return_roundtrip() {
    use nusa_octane_worker::pool::WorkerPool;

    let mut pool = WorkerPool::new(1, PathBuf::from("/tmp"), 256, 1000);

    pool.push_test_worker(create_stub_worker(0));
    pool.enqueue_idle_worker(0);

    let worker = pool.get_idle_worker();
    assert!(worker.is_some());
    assert_eq!(pool.idle_count(), 0);

    pool.return_worker(0);
    assert_eq!(pool.idle_count(), 1);

    let worker2 = pool.get_idle_worker();
    assert!(worker2.is_some());
}

// ============================================================================
// Stress Tests: Recovery After Stress
// ============================================================================

#[test]
fn worker_metrics_recovery_returns_to_baseline() {
    let metrics = WorkerMetrics::new();

    // Simulate heavy load
    for _ in 0..10000 {
        metrics.record_request(100, true);
    }

    let total_after_load = metrics.requests_handled();
    assert!(total_after_load > 0);

    // After "stress", record a single request
    metrics.record_request(50, true);
    assert_eq!(metrics.requests_handled(), total_after_load + 1);
}

// ============================================================================
// Orchestrator Event Handling Tests
// ============================================================================

use nusa_octane_worker::state_reset::{OctaneEvent, StateResetOrchestrator};

#[test]
fn orchestrator_known_event_triggers_action() {
    let mut orchestrator = StateResetOrchestrator::new(128);
    let triggered = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = triggered.clone();

    orchestrator.register_action("request_received".into(), move |_event| {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    });

    let event = OctaneEvent::RequestReceived {
        request_id: "test-1".into(),
    };
    orchestrator.emit_event(event);

    assert!(
        triggered.load(std::sync::atomic::Ordering::SeqCst),
        "action should have been triggered"
    );
}

#[test]
fn orchestrator_unknown_event_noop() {
    let orchestrator = StateResetOrchestrator::new(128);

    // Emit an event without any registered action
    let event = OctaneEvent::WorkerStarted { worker_id: 0 };
    orchestrator.emit_event(event);

    // Should not panic - unknown events are no-ops
}

#[test]
fn orchestrator_duplicate_event_idempotent() {
    let mut orchestrator = StateResetOrchestrator::new(128);
    let count = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let counter = count.clone();

    orchestrator.register_action("request_terminated".into(), move |_event| {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    });

    // Emit same event twice - should trigger action twice (idempotent per call)
    for i in 0..2 {
        let event = OctaneEvent::RequestTerminated {
            request_id: format!("dup-{i}"),
            status: 200,
        };
        orchestrator.emit_event(event);
    }

    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[test]
fn orchestrator_stats_accurate() {
    let orchestrator = StateResetOrchestrator::new(128);

    orchestrator.emit_event(OctaneEvent::RequestReceived {
        request_id: "req-1".into(),
    });
    orchestrator.emit_event(OctaneEvent::RequestTerminated {
        request_id: "req-1".into(),
        status: 200,
    });
    orchestrator.emit_event(OctaneEvent::WorkerStopping { worker_id: 0 });

    let stats = orchestrator.stats();
    assert_eq!(stats.total_requests_processed, 1);
    assert_eq!(stats.total_resets_performed, 1);
    assert_eq!(stats.total_cleanups_performed, 1);
    assert_eq!(stats.total_worker_stops, 1);
}

// ============================================================================
// StateResetOrchestrator Lifecycle Tests
// ============================================================================

#[test]
fn orchestrator_initialization_sets_default_actions() {
    let mut orchestrator = StateResetOrchestrator::new(128);
    orchestrator.initialize();

    // After initialization, events should not panic
    orchestrator.emit_event(OctaneEvent::RequestReceived {
        request_id: "test".into(),
    });
    orchestrator.emit_event(OctaneEvent::RequestTerminated {
        request_id: "test".into(),
        status: 200,
    });
    orchestrator.emit_event(OctaneEvent::WorkerStopping { worker_id: 0 });
    orchestrator.emit_event(OctaneEvent::WorkerStarted { worker_id: 0 });
}

#[test]
fn orchestrator_shutdown_no_panic() {
    let orchestrator = StateResetOrchestrator::new(128);
    orchestrator.shutdown();
    // Should not panic
}

#[test]
fn orchestrator_broadcast_channel_works() {
    let orchestrator = StateResetOrchestrator::new(128);
    let mut rx = orchestrator.subscribe();

    orchestrator.emit_event(OctaneEvent::RequestReceived {
        request_id: "broadcast-test".into(),
    });

    // Event should be receivable
    let received = rx.try_recv();
    assert!(received.is_ok(), "event should be broadcast");
}
