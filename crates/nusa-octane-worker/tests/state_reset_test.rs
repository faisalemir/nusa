//! State reset orchestrator tests.
//! Tests OctaneEvent lifecycle, stats tracking, and action registration.

use std::time::Duration;

use nusa_octane_worker::state_reset::{OctaneEvent, StateResetOrchestrator};

#[tokio::test]
async fn state_reset_orchestrator_initial_stats_zero() {
    let orchestrator = StateResetOrchestrator::new(128);
    let stats = orchestrator.stats();

    assert_eq!(stats.total_requests_processed, 0);
    assert_eq!(stats.total_resets_performed, 0);
    assert_eq!(stats.total_cleanups_performed, 0);
    assert_eq!(stats.total_worker_stops, 0);
}

#[tokio::test]
async fn state_reset_orchestrator_request_received_increments() {
    let mut orchestrator = StateResetOrchestrator::new(128);
    orchestrator.initialize();

    orchestrator
        .emit_event(OctaneEvent::RequestReceived {
            request_id: "req-1".to_string(),
        })
        .unwrap();

    let stats = orchestrator.stats();
    assert_eq!(stats.total_requests_processed, 1);
    assert_eq!(stats.total_resets_performed, 0);
}

#[tokio::test]
async fn state_reset_orchestrator_request_terminated_increments_resets() {
    let mut orchestrator = StateResetOrchestrator::new(128);
    orchestrator.initialize();

    orchestrator
        .emit_event(OctaneEvent::RequestTerminated {
            request_id: "req-1".to_string(),
            status: 200,
        })
        .unwrap();

    let stats = orchestrator.stats();
    assert_eq!(stats.total_resets_performed, 1);
    assert_eq!(stats.total_cleanups_performed, 1);
}

#[tokio::test]
async fn state_reset_orchestrator_worker_stopping_increments_stops() {
    let mut orchestrator = StateResetOrchestrator::new(128);
    orchestrator.initialize();

    orchestrator
        .emit_event(OctaneEvent::WorkerStopping { worker_id: 0 })
        .unwrap();

    let stats = orchestrator.stats();
    assert_eq!(stats.total_worker_stops, 1);
}

#[tokio::test]
async fn state_reset_orchestrator_multiple_events_accumulate() {
    let mut orchestrator = StateResetOrchestrator::new(128);
    orchestrator.initialize();

    // 5 requests received
    for i in 0..5 {
        orchestrator
            .emit_event(OctaneEvent::RequestReceived {
                request_id: format!("req-{}", i),
            })
            .unwrap();
    }

    // 3 requests terminated
    for i in 0..3 {
        orchestrator
            .emit_event(OctaneEvent::RequestTerminated {
                request_id: format!("req-{}", i),
                status: 200,
            })
            .unwrap();
    }

    // 2 workers stopping
    for i in 0..2 {
        orchestrator
            .emit_event(OctaneEvent::WorkerStopping { worker_id: i })
            .unwrap();
    }

    let stats = orchestrator.stats();
    assert_eq!(stats.total_requests_processed, 5);
    assert_eq!(stats.total_resets_performed, 3);
    assert_eq!(stats.total_cleanups_performed, 3);
    assert_eq!(stats.total_worker_stops, 2);
}

#[tokio::test]
async fn state_reset_orchestrator_broadcast_subscribers_receive() {
    let mut orchestrator = StateResetOrchestrator::new(128);
    orchestrator.initialize();

    let mut subscriber = orchestrator.subscribe();

    orchestrator
        .emit_event(OctaneEvent::RequestReceived {
            request_id: "req-broadcast".to_string(),
        })
        .unwrap();

    let received = tokio::time::timeout(Duration::from_millis(100), subscriber.recv())
        .await
        .expect("timeout waiting for broadcast")
        .expect("channel closed");

    match received {
        OctaneEvent::RequestReceived { request_id } => {
            assert_eq!(request_id, "req-broadcast");
        }
        other => panic!("expected RequestReceived, got {:?}", other),
    }
}

#[tokio::test]
async fn state_reset_orchestrator_custom_action_executed() {
    use std::sync::atomic::{AtomicU64, Ordering};

    let counter = std::sync::Arc::new(AtomicU64::new(0));
    let counter_clone = counter.clone();

    let mut orchestrator = StateResetOrchestrator::new(128);
    orchestrator.register_action("request_received".to_string(), move |_event| {
        counter_clone.fetch_add(1, Ordering::SeqCst);
    });

    orchestrator
        .emit_event(OctaneEvent::RequestReceived {
            request_id: "req-custom".to_string(),
        })
        .unwrap();

    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn state_reset_orchestrator_shutdown_no_panic() {
    let orchestrator = StateResetOrchestrator::new(128);
    orchestrator.shutdown();
}

#[tokio::test]
async fn state_reset_orchestrator_concurrent_emits() {
    let mut orchestrator = StateResetOrchestrator::new(1024);
    orchestrator.initialize();

    let orchestrator = std::sync::Arc::new(orchestrator);

    // 10 threads each emit 100 events
    let mut handles = Vec::new();
    for thread_id in 0..10 {
        let o = orchestrator.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..100 {
                o.emit_event(OctaneEvent::RequestReceived {
                    request_id: format!("t{}-r{}", thread_id, i),
                })
                .unwrap();
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let stats = orchestrator.stats();
    assert_eq!(stats.total_requests_processed, 1000);
}

#[tokio::test]
async fn state_reset_orchestrator_multiple_subscribers() {
    let mut orchestrator = StateResetOrchestrator::new(128);
    orchestrator.initialize();

    let mut sub1 = orchestrator.subscribe();
    let mut sub2 = orchestrator.subscribe();
    let mut sub3 = orchestrator.subscribe();

    orchestrator
        .emit_event(OctaneEvent::RequestReceived {
            request_id: "req-multi".to_string(),
        })
        .unwrap();

    // All subscribers should receive
    for (i, sub) in [&mut sub1, &mut sub2, &mut sub3].iter_mut().enumerate() {
        let received = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .expect(format!("subscriber {} timeout", i).as_str())
            .expect("channel closed");
        assert!(matches!(received, OctaneEvent::RequestReceived { .. }));
    }
}
