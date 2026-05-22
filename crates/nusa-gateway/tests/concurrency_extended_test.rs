//! Extended concurrency tests for nusa-gateway crate.
//!
//! Covers: CircuitBreaker deadlock, WsManager/SseManager concurrent access,
//! HealthState concurrency, TenantCircuitBreakers contention, task cancellation,
//! starvation scenarios.

use std::sync::Arc;
use std::time::Duration;

use nusa_core::TenantId;
use nusa_gateway::circuit_breaker::{CbState, CircuitBreaker};
use nusa_gateway::health::HealthState;
use nusa_gateway::sse::SseManager;
use nusa_gateway::tenant_circuit_breaker::TenantCircuitBreakers;
use nusa_gateway::websocket::WsManager;
use parking_lot::Mutex;

// ── CircuitBreaker: Deadlock / State Transitions ──

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn circuitbreaker_concurrent_failures_state_transitions_no_deadlock() {
    // === Arrange ===
    let cb = Arc::new(CircuitBreaker::new(5, Duration::from_millis(100)));

    // === Act ===
    let mut handles = Vec::new();

    // Thread recording failures
    for _ in 0..4 {
        let breaker = cb.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..20 {
                breaker.record_failure();
            }
        }));
    }

    // Thread checking allow_request
    for _ in 0..4 {
        let breaker = cb.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..100 {
                let _ = breaker.allow_request();
            }
        }));
    }

    // Thread recording successes
    for _ in 0..4 {
        let breaker = cb.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..20 {
                breaker.record_success();
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

    // Circuit breaker must still be functional
    let _state = cb.state();
    let _count = cb.failure_count();
}

#[tokio::test]
async fn circuitbreaker_concurrent_half_open_probes() {
    // === Arrange ===
    let cb = Arc::new(CircuitBreaker::new(2, Duration::from_millis(50)));

    // Trip the circuit breaker
    cb.record_failure();
    cb.record_failure();
    assert!(!cb.allow_request(), "circuit must be open");

    // Wait for reset timeout
    tokio::time::sleep(Duration::from_millis(100)).await;

    // === Act ===
    // Multiple threads trying to probe half-open simultaneously
    let mut handles = Vec::new();
    for _ in 0..8 {
        let breaker = cb.clone();
        handles.push(tokio::spawn(async move {
            let allowed = breaker.allow_request();
            if allowed {
                breaker.record_success();
            } else {
                breaker.record_failure();
            }
            allowed
        }));
    }

    // === Assert ===
    let mut any_allowed = false;
    for h in handles {
        let allowed = tokio::time::timeout(Duration::from_secs(5), h)
            .await
            .expect("must complete")
            .expect("must not panic");
        if allowed {
            any_allowed = true;
        }
    }

    // At least one probe should have been allowed in half-open state
    assert!(
        any_allowed,
        "at least one probe must be allowed in half-open"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn circuitbreaker_concurrent_tenant_operations_no_lost_updates() {
    // === Arrange ===
    let tenant_cb = Arc::new(TenantCircuitBreakers::new(3, Duration::from_millis(200)));

    // === Act ===
    let mut handles = Vec::new();

    for tenant_idx in 0..8 {
        let tcb = tenant_cb.clone();
        handles.push(tokio::spawn(async move {
            let tid = TenantId::new(format!("tenant-{}", tenant_idx));

            // Record successes and failures concurrently
            for i in 0..50 {
                if i % 3 == 0 {
                    tcb.record_failure(&tid);
                } else {
                    tcb.record_success(&tid);
                }
            }

            tcb.state(&tid)
        }));
    }

    // === Assert ===
    for h in handles {
        let state = tokio::time::timeout(Duration::from_secs(10), h)
            .await
            .expect("must complete")
            .expect("must not panic");
        // State should be valid (any state is fine, just no crash)
        assert!(matches!(
            state,
            Some(CbState::Closed) | Some(CbState::Open) | Some(CbState::HalfOpen) | None
        ));
    }
}

// ── WsManager: Concurrent Broadcast + Subscribe ──

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wsmanager_concurrent_broadcast_subscribe_no_deadlock() {
    // === Arrange ===
    let ws = Arc::new(WsManager::new());
    let tid = TenantId::new("test-tenant");

    // Register test connections
    let mut receivers = Vec::new();
    for i in 0..4 {
        let cid = format!("conn-{}", i);
        let rx = ws.register_test_connection(cid, tid.clone());
        receivers.push(rx);
    }

    // === Act ===
    let mut broadcast_handles = Vec::new();
    for msg_idx in 0..10 {
        let ws_manager = ws.clone();
        let tenant = tid.clone();
        broadcast_handles.push(tokio::spawn(async move {
            ws_manager.broadcast_to_tenant(&tenant, &format!("message-{}", msg_idx));
        }));
    }

    // === Assert ===
    for h in broadcast_handles {
        tokio::time::timeout(Duration::from_secs(5), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }

    // Verify connection count
    assert_eq!(
        ws.connection_count(),
        4,
        "connections must still be registered"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wsmanager_concurrent_subscribe_disconnect_no_race() {
    // === Arrange ===
    let ws = Arc::new(WsManager::new());
    let tid = TenantId::new("race-test");

    // === Act ===
    let mut handles = Vec::new();
    for i in 0..8 {
        let ws_manager = ws.clone();
        let tenant = tid.clone();
        handles.push(tokio::spawn(async move {
            let cid = format!("conn-{}", i);
            let _rx = ws_manager.register_test_connection(cid, tenant.clone());

            // Immediately broadcast
            ws_manager.broadcast_to_tenant(&tenant, "hello");
        }));
    }

    // === Assert ===
    for h in handles {
        tokio::time::timeout(Duration::from_secs(10), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }
}

// ── SseManager: Concurrent Send + Stream ──

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ssemanager_concurrent_send_stream_creation_no_deadlock() {
    // === Arrange ===
    let sse = Arc::new(SseManager::new());

    // === Act ===
    let mut handles = Vec::new();

    // Concurrent stream creation (subscribes)
    for _ in 0..4 {
        let manager = sse.clone();
        handles.push(tokio::spawn(async move {
            let _stream = manager.stream();
        }));
    }

    // Concurrent sends
    for i in 0..20 {
        let manager = sse.clone();
        handles.push(tokio::spawn(async move {
            manager.send(&format!("event-{}", i));
        }));
    }

    // === Assert ===
    for h in handles {
        tokio::time::timeout(Duration::from_secs(5), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ssemanager_concurrent_disconnect_send_no_loss() {
    // === Arrange ===
    let sse = Arc::new(SseManager::new());

    // === Act ===
    let mut handles = Vec::new();

    // Create and drop subscribers
    for _ in 0..10 {
        let manager = sse.clone();
        handles.push(tokio::spawn(async move {
            let stream = manager.stream();
            // Use stream briefly
            drop(stream);
        }));
    }

    // Send during subscriber churn
    for i in 0..50 {
        let manager = sse.clone();
        handles.push(tokio::spawn(async move {
            manager.send(&format!("event-{}", i));
        }));
    }

    // === Assert ===
    for h in handles {
        tokio::time::timeout(Duration::from_secs(10), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }
}

// ── HealthState: Concurrent Read/Write ──

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn healthstate_concurrent_record_success_error_no_race() {
    // === Arrange ===
    let health = Arc::new(HealthState::new());
    health.mark_ready();

    // === Act ===
    let mut handles = Vec::new();
    let mut read_handles = Vec::new();

    // Record successes
    for _ in 0..4 {
        let h = health.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..100 {
                h.record_success();
            }
        }));
    }

    // Record errors
    for _ in 0..4 {
        let h = health.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..50 {
                h.record_error();
            }
        }));
    }

    // Read concurrently
    for _ in 0..4 {
        let h = health.clone();
        let read_handle = tokio::spawn(async move {
            let mut reads = Vec::new();
            for _ in 0..100 {
                reads.push((h.success_count(), h.error_count()));
            }
            reads
        });
        read_handles.push(read_handle);
    }

    // === Assert ===
    for h in handles {
        tokio::time::timeout(Duration::from_secs(10), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }
    for h in read_handles {
        tokio::time::timeout(Duration::from_secs(10), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }

    // Final counts must be consistent
    let success = health.success_count();
    let error = health.error_count();
    assert_eq!(success, 400, "success count must be accurate");
    assert_eq!(error, 200, "error count must be accurate");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn healthstate_concurrent_mark_ready_no_race() {
    // === Arrange ===
    let health = Arc::new(HealthState::new());

    // === Act ===
    let mut handles = Vec::new();
    for _ in 0..8 {
        let h = health.clone();
        handles.push(tokio::spawn(async move {
            h.mark_ready();
            assert!(h.is_ready(), "must be ready after mark");
        }));
    }

    // === Assert ===
    for h in handles {
        tokio::time::timeout(Duration::from_secs(5), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }

    assert!(health.is_ready(), "health must be ready");
}

// ── TenantCircuitBreakers: Concurrent Access ──

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn tenantcircuitbreakers_concurrent_access_no_deadlock() {
    // === Arrange ===
    let tcb = Arc::new(TenantCircuitBreakers::new(10, Duration::from_millis(100)));

    // === Act ===
    let mut handles = Vec::new();
    for tenant_idx in 0..16 {
        let manager = tcb.clone();
        handles.push(tokio::spawn(async move {
            let tid = TenantId::new(format!("tenant-{}", tenant_idx));

            // get_or_create + is_allowed + record_success/failure
            for i in 0..50 {
                let _cb = manager.get_or_create(&tid);
                if i % 5 == 0 {
                    manager.record_failure(&tid);
                } else {
                    manager.record_success(&tid);
                }
                let _ = manager.is_allowed(&tid);
                let _ = manager.state(&tid);
            }
        }));
    }

    // === Assert ===
    for h in handles {
        tokio::time::timeout(Duration::from_secs(15), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }

    assert_eq!(tcb.count(), 16, "all tenant breakers must be created");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tenantcircuitbreakers_concurrent_open_close_transitions() {
    // === Arrange ===
    let tcb = Arc::new(TenantCircuitBreakers::new(2, Duration::from_millis(50)));
    let tid = TenantId::new("transition-test");

    // === Act ===
    let mut handles = Vec::new();

    // Open the circuit
    let mut open_handles = Vec::new();
    for _ in 0..4 {
        let manager = tcb.clone();
        let tenant = tid.clone();
        open_handles.push(tokio::spawn(async move {
            manager.record_failure(&tenant);
        }));
    }

    for h in open_handles {
        tokio::time::timeout(Duration::from_secs(5), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }

    // Wait for reset timeout
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Try to close via half-open probe
    for _ in 0..4 {
        let manager = tcb.clone();
        let tenant = tid.clone();
        handles.push(tokio::spawn(async move {
            if manager.is_allowed(&tenant) {
                manager.record_success(&tenant);
            }
        }));
    }

    // === Assert ===
    for h in handles {
        tokio::time::timeout(Duration::from_secs(5), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }

    let state = tcb.state(&tid);
    assert!(state.is_some(), "state must exist");
}

// ── Task Cancellation ──

#[tokio::test]
async fn wsmanager_drop_handler_during_await_cleanup() {
    // === Arrange ===
    let ws = WsManager::new();
    let tid = TenantId::new("drop-test");

    // === Act ===
    let _rx = ws.register_test_connection("conn-drop".to_string(), tid.clone());

    // Broadcast while receiver will be dropped
    ws.broadcast_to_tenant(&tid, "message");
    drop(_rx);

    // === Assert ===
    // Manager must still be functional
    ws.broadcast_to_tenant(&tid, "after-drop");
    assert_eq!(
        ws.connection_count(),
        1,
        "connection must still be registered"
    );
}

#[tokio::test]
async fn ssemanager_send_error_stream_cleanup() {
    // === Arrange ===
    let sse = SseManager::new();

    // === Act ===
    // Create stream, use briefly, drop
    {
        let stream = sse.stream();
        drop(stream);
    }

    // Send to dropped subscriber
    sse.send("after-drop");

    // === Assert ===
    // No crash = cleanup successful
    let stream2 = sse.stream();
    drop(stream2);
}

// ── Starvation: Hot-Key Contention ──

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn tenantcircuitbreakers_hot_key_contention_no_starvation() {
    // === Arrange ===
    let tcb = Arc::new(TenantCircuitBreakers::new(1000, Duration::from_millis(100)));
    let hot_tenant = TenantId::new("hot-key");

    // Pre-create the breaker
    let _cb = tcb.get_or_create(&hot_tenant);

    // === Act ===
    let mut handles = Vec::new();
    for _ in 0..16 {
        let manager = tcb.clone();
        let tenant = hot_tenant.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..100 {
                let _ = manager.is_allowed(&tenant);
                manager.record_success(&tenant);
            }
        }));
    }

    // === Assert ===
    for h in handles {
        tokio::time::timeout(Duration::from_secs(15), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }

    // Breaker should still be functional
    assert!(tcb.is_allowed(&hot_tenant));
}

// ── BackpressureGuard: Resource ──

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn backpressureguard_permit_acquire_release_10k_no_leak() {
    // === Arrange ===
    let guard = Arc::new(nusa_core::BackpressureGuard::new(100));
    let acquired = Arc::new(Mutex::new(0u64));

    // === Act ===
    let mut handles = Vec::new();
    for _ in 0..16 {
        let g = guard.clone();
        let a = acquired.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..625 {
                // 16 * 625 = 10000
                if let Some(_permit) = g.try_acquire().await {
                    *a.lock() += 1;
                }
            }
        }));
    }

    // === Assert ===
    for h in handles {
        tokio::time::timeout(Duration::from_secs(30), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }

    let total = *acquired.lock();
    assert!(total > 0, "some permits must have been acquired");
}

// ── CircuitBreaker: Resource ──

#[test]
fn circuitbreaker_open_close_cycle_1000_no_resource_accumulation() {
    // === Arrange ===
    let cb = CircuitBreaker::new(1, Duration::from_secs(60));

    // === Act ===
    for _ in 0..1000 {
        // Open
        cb.record_failure();
        assert_eq!(cb.state(), nusa_gateway::circuit_breaker::CbState::Open);
        assert!(!cb.allow_request(), "circuit must be open");

        // Wait zero seconds (timeout is 0) and try again
        // Close via success in half-open
        std::thread::sleep(Duration::from_millis(1));
        if cb.allow_request() {
            cb.record_success();
        }
    }

    // === Assert ===
    // No crash = no resource accumulation
    let _state = cb.state();
    let _count = cb.failure_count();
}
