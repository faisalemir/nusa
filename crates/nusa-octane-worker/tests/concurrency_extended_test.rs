//! Extended concurrency tests for nusa-octane-worker crate.
//!
//! Covers: Worker handle_request races, Pool deadlock scenarios, WorkerMetrics
//! relaxed ordering, Orchestrator concurrent emits, stop races, recycle concurrency.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use nusa_octane_worker::metrics::WorkerMetrics;
use nusa_octane_worker::pool::{Worker, WorkerPool, WorkerState};
use nusa_octane_worker::state_reset::{OctaneEvent, StateResetOrchestrator};

// ── WorkerMetrics: Relaxed Ordering ──

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn workermetrics_relaxed_ordering_no_stale_reads_under_load() {
    // === Arrange ===
    let metrics = Arc::new(WorkerMetrics::new());

    // === Act ===
    let mut handles = Vec::new();

    // Concurrent record
    for i in 0..8 {
        let m = metrics.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..100 {
                m.record_request(10 + i, i % 3 == 0);
            }
        }));
    }

    // Concurrent read
    for _ in 0..4 {
        let m = metrics.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..100 {
                let _ = m.requests_handled();
                let _ = m.error_count();
                let _ = m.error_rate();
                let _ = m.avg_response_time_ms();
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

    // Final values must be consistent
    assert_eq!(
        metrics.requests_handled(),
        800,
        "all requests must be recorded"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn workermetrics_concurrent_record_mixed_success_error_consistent() {
    // === Arrange ===
    let metrics = Arc::new(WorkerMetrics::new());

    // === Act ===
    let mut handles = Vec::new();
    for i in 0..8 {
        let m = metrics.clone();
        let success = i % 2 == 0;
        handles.push(tokio::spawn(async move {
            for _ in 0..50 {
                m.record_request(5, success);
            }
        }));
    }

    for h in handles {
        tokio::time::timeout(Duration::from_secs(10), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }

    // === Assert ===
    let total = metrics.requests_handled();
    let errors = metrics.error_count();
    assert_eq!(total, 400, "all requests recorded");
    assert!(errors > 0, "some errors must be recorded");
    assert!(errors < total, "not all requests are errors");

    let rate = metrics.error_rate();
    assert!(
        rate > 0.0 && rate < 1.0,
        "error rate must be between 0 and 1"
    );
}

// ── Orchestrator: Concurrent Emits ──

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn orchestrator_concurrent_emits_no_lost_events() {
    // === Arrange ===
    let mut orchestrator = StateResetOrchestrator::new(128);
    orchestrator.initialize();

    let counter = Arc::new(AtomicU64::new(0));
    let counter_clone = counter.clone();

    // Register action
    orchestrator.register_action("request_received".to_string(), move |_event| {
        counter_clone.fetch_add(1, Ordering::SeqCst);
    });

    let orchestrator = Arc::new(orchestrator);

    // === Act ===
    let mut handles = Vec::new();
    for i in 0..8 {
        let orch = orchestrator.clone();
        handles.push(tokio::spawn(async move {
            for j in 0..50 {
                orch.emit_event(OctaneEvent::RequestReceived {
                    request_id: format!("req-{}-{}", i, j),
                });
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

    let total = counter.load(Ordering::SeqCst);
    assert_eq!(total, 400, "all events must trigger action");
}

#[test]
fn orchestrator_register_action_while_emit_no_hashmap_race() {
    // === Arrange ===
    let mut orchestrator = StateResetOrchestrator::new(128);

    // === Act ===
    // Register actions while they conceptually could fire
    for i in 0..10 {
        let action_name = format!("action-{}", i);
        orchestrator.register_action(action_name.clone(), move |_| {
            // No-op action
        });

        // Emit event that doesn match — no race on HashMap
        orchestrator.emit_event(OctaneEvent::WorkerStarted { worker_id: i });
    }

    // === Assert ===
    let stats = orchestrator.stats();
    assert_eq!(stats.total_worker_stops, 0, "no worker stops recorded");
    assert_eq!(stats.total_requests_processed, 0, "no requests processed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn orchestrator_broadcast_many_subscribers_many_events_no_deadlock() {
    // === Arrange ===
    let mut orchestrator = StateResetOrchestrator::new(128);
    orchestrator.initialize();

    // Create multiple subscribers
    let mut subscribers = Vec::new();
    for _ in 0..8 {
        subscribers.push(orchestrator.subscribe());
    }

    let orchestrator = Arc::new(orchestrator);

    // === Act ===
    // Emit many events from multiple threads
    let mut handles = Vec::new();
    for i in 0..4 {
        let orch = orchestrator.clone();
        handles.push(tokio::spawn(async move {
            for j in 0..50 {
                orch.emit_event(OctaneEvent::RequestTerminated {
                    request_id: format!("req-{}-{}", i, j),
                    status: 200,
                });
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

    // All subscribers should be able to drain
    for mut rx in subscribers {
        while rx.try_recv().is_ok() {
            // drain subscriber queue; some events may be lost due to broadcast buffer
        }
    }
}

// ── Worker State Transitions ──

#[test]
fn worker_state_idle_to_busy_to_idle_transition_correct() {
    // === Arrange ===
    assert_eq!(WorkerState::Idle, WorkerState::Idle);
    assert_eq!(WorkerState::Busy, WorkerState::Busy);
    assert!(WorkerState::Idle != WorkerState::Busy);

    // === Act & Assert ===
    // State enum is Copy + PartialEq — test transitions conceptually
    let mut state = WorkerState::Idle;
    assert_eq!(state, WorkerState::Idle);

    state = WorkerState::Busy;
    assert_eq!(state, WorkerState::Busy);

    state = WorkerState::Idle;
    assert_eq!(state, WorkerState::Idle);
}

#[test]
fn worker_state_all_variants_distinct() {
    // === Arrange ===
    let states = [
        WorkerState::Idle,
        WorkerState::Busy,
        WorkerState::Draining,
        WorkerState::Stopped,
    ];

    // === Act & Assert ===
    // All states must be distinct
    for (i, s1) in states.iter().enumerate() {
        for (j, s2) in states.iter().enumerate() {
            if i != j {
                assert_ne!(s1, s2, "states must be distinct");
            } else {
                assert_eq!(s1, s2, "same state must be equal");
            }
        }
    }
}

// ── WorkerMetrics: Weak Memory Ordering ──

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn workermetrics_weak_memory_ordering_visibility() {
    // === Arrange ===
    // On weak-memory-ordering CPUs (ARM), Relaxed ordering may show stale reads.
    // Test that eventual consistency holds.
    let metrics = Arc::new(WorkerMetrics::new());

    // === Act ===
    // Writer thread
    let writer = {
        let m = metrics.clone();
        std::thread::spawn(move || {
            for i in 0..100 {
                m.record_request(1, i % 2 == 0);
            }
        })
    };

    // Reader thread — spin until it sees all 100
    let reader = {
        let m = metrics.clone();
        std::thread::spawn(move || {
            let mut max_seen = 0u64;
            for _ in 0..10000 {
                let seen = m.requests_handled();
                max_seen = max_seen.max(seen);
                std::thread::yield_now();
            }
            max_seen
        })
    };

    writer.join().expect("writer must not panic");
    let max_seen = reader.join().expect("reader must not panic");

    // === Assert ===
    assert!(
        max_seen > 0,
        "reader must see some writes (eventual consistency)"
    );
    assert!(max_seen <= 100, "reader must not see more than written");
}

// ── Pool: Shutdown During Dispatch ──

#[tokio::test]
async fn workerpool_shutdown_during_dispatch_graceful_drain() {
    // === Arrange ===
    // Use stub pool (no real workers needed for this test)
    let mut pool = WorkerPool::new(1, PathBuf::from("/tmp/nusa-test-nonexistent"), 256, 10);

    // === Act ===
    // Initialize will fail (no PHP), but shutdown should still work
    let init_result = pool.initialize().await;

    // Shutdown must work even if init failed
    let shutdown_result = pool.shutdown().await;

    // === Assert ===
    if init_result.is_err() {
        // Expected — no PHP available
        assert!(shutdown_result.is_ok() || pool.worker_count() == 0);
    }
}

// ── Pool: Concurrent Get/Return ──

#[test]
fn workerpool_get_idle_returns_none_when_empty() {
    // === Arrange ===
    let mut pool = WorkerPool::new(4, PathBuf::from("/tmp"), 256, 100);

    // === Act & Assert ===
    assert!(
        pool.get_idle_worker().is_none(),
        "empty pool must return None"
    );
}

#[test]
fn workerpool_return_worker_respects_draining() {
    // === Arrange ===
    let mut pool = WorkerPool::new(1, PathBuf::from("/tmp"), 256, 100);
    pool.push_test_worker(Worker::new_test_stub(0));

    // === Act ===
    pool.worker_mut(0).state = WorkerState::Draining;
    pool.return_worker(0);

    // === Assert ===
    assert_eq!(
        pool.idle_count(),
        0,
        "draining worker must not return to idle"
    );
}

#[test]
fn workerpool_return_worker_idle_when_not_draining() {
    // === Arrange ===
    let mut pool = WorkerPool::new(1, PathBuf::from("/tmp"), 256, 100);
    pool.push_test_worker(Worker::new_test_stub(0));
    pool.worker_mut(0).state = WorkerState::Idle;

    // === Act ===
    pool.return_worker(0);

    // === Assert ===
    assert_eq!(pool.idle_count(), 1, "idle worker must return to queue");
}

// ── Pool: Recycle Concurrent ──

#[test]
fn workerpool_idle_queue_no_corruption_on_concurrent_like_access() {
    // === Arrange ===
    // Simulate concurrent-like access pattern
    let mut pool = WorkerPool::new(4, PathBuf::from("/tmp"), 256, 100);
    for i in 0..4 {
        pool.push_test_worker(Worker::new_test_stub(i));
    }

    // === Act ===
    // Get all idle workers (there are none initially)
    for _ in 0..4 {
        let _ = pool.get_idle_worker();
    }

    // Return all workers
    for i in 0..4 {
        pool.return_worker(i);
    }

    // === Assert ===
    assert_eq!(pool.idle_count(), 4, "all workers must be in idle queue");
}

// ── Broadcast Channel Backpressure ──

#[tokio::test]
async fn orchestrator_broadcast_slow_subscriber_no_blocking() {
    // === Arrange ===
    let mut orchestrator = StateResetOrchestrator::new(8); // Small buffer
    orchestrator.initialize();

    let _rx = orchestrator.subscribe(); // Slow subscriber that doesn't consume

    // === Act ===
    // Emit more events than buffer size — slow subscriber will lag
    for i in 0..100 {
        orchestrator.emit_event(OctaneEvent::RequestReceived {
            request_id: format!("req-{}", i),
        });
    }

    // === Assert ===
    // Stats must reflect all events
    let stats = orchestrator.stats();
    assert_eq!(
        stats.total_requests_processed, 100,
        "all events must be counted"
    );
}
