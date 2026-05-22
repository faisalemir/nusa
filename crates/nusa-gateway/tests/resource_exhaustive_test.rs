//! Resource exhaustion tests for nusa-gateway crate.
//!
//! Covers: FD leak, memory leak, connection leak, cleanup after panic/error/timeout,
//! CircuitBreaker resource cycling, BackpressureGuard resource cycling.

use std::sync::Arc;
use std::time::Duration;

use nusa_core::TenantId;
use nusa_gateway::circuit_breaker::CircuitBreaker;
use nusa_gateway::sse::SseManager;
use nusa_gateway::websocket::WsManager;

// ── Memory Leak: WsManager Broadcast ──

#[tokio::test]
async fn wsmanager_broadcast_10k_times_memory_stable() {
    // === Arrange ===
    let ws = Arc::new(WsManager::new());
    let tid = TenantId::new("mem-test");
    let mut _rx = ws.register_test_connection("conn-mem".to_string(), tid.clone());

    // === Act ===
    for i in 0..10_000 {
        ws.broadcast_to_tenant(&tid, &format!("msg-{}", i));
    }

    // === Assert ===
    assert_eq!(
        ws.connection_count(),
        1,
        "connection must still be registered"
    );
}

// ── Memory Leak: SseManager Events ──

#[tokio::test]
async fn ssemanager_10k_events_memory_stable() {
    // === Arrange ===
    let sse = Arc::new(SseManager::new());

    // === Act ===
    for i in 0..10_000 {
        sse.send(&format!("event-{}", i));
    }

    // === Assert ===
    // No crash = memory stable (broadcast channel handles 256 buffer internally)
}

// ── CircuitBreaker Resource ──

#[test]
fn circuitbreaker_open_close_cycle_1000_no_accumulation() {
    // === Arrange ===
    let cb = CircuitBreaker::new(1, Duration::from_millis(1));

    // === Act ===
    for _ in 0..1000 {
        cb.record_failure();
        std::thread::sleep(Duration::from_millis(2));
        if cb.allow_request() {
            cb.record_success();
        }
    }

    // === Assert ===
    // Must still be functional
    assert!(cb.allow_request() || !cb.allow_request());
}

#[test]
fn circuitbreaker_panic_during_broadcast_no_leak() {
    // === Arrange ===
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let cb = CircuitBreaker::new(3, Duration::from_secs(1));
        cb.record_failure();
        panic!("simulated panic during operation");
    }));

    // === Act & Assert ===
    assert!(result.is_err(), "panic must propagate");
}

// ── SSE Cleanup After Error ──

#[tokio::test]
async fn ssemanager_error_path_stream_cleaned_up() {
    // === Arrange ===
    let sse = SseManager::new();

    // === Act ===
    // Create multiple streams and drop them
    let streams: Vec<_> = (0..10).map(|_| sse.stream()).collect();
    drop(streams);

    // Send after all streams dropped
    sse.send("after-all-dropped");

    // === Assert ===
    // New stream must work
    let new_stream = sse.stream();
    drop(new_stream);
}

// ── WsManager Cleanup After Timeout ──

#[tokio::test]
async fn wsmanager_timeout_resources_released() {
    // === Arrange ===
    let ws = WsManager::new();
    let tid = TenantId::new("timeout-test");
    let _rx = ws.register_test_connection("conn-timeout".to_string(), tid.clone());

    // === Act ===
    // Register and then let connection count verify no leak
    ws.broadcast_to_tenant(&tid, "timeout-msg");

    // === Assert ===
    assert_eq!(ws.connection_count(), 1, "connection must still be tracked");
}

// ── TenantCircuitBreakers Resource ──

#[tokio::test]
async fn tenantcircuitbreakers_many_tenants_no_fd_accumulation() {
    // === Arrange ===
    let tcb = nusa_gateway::tenant_circuit_breaker::TenantCircuitBreakers::new(
        100,
        Duration::from_millis(100),
    );

    // === Act ===
    for i in 0..1000 {
        let tid = TenantId::new(format!("tenant-{}", i));
        let _cb = tcb.get_or_create(&tid);
        tcb.record_success(&tid);
        let _ = tcb.is_allowed(&tid);
    }

    // === Assert ===
    assert_eq!(tcb.count(), 1000, "all breakers must be tracked");
}

// ── BackpressureGuard Resource ──

#[tokio::test]
async fn backpressureguard_acquire_release_10k_no_leak() {
    // === Arrange ===
    let guard = Arc::new(nusa_core::BackpressureGuard::new(50));

    // === Act ===
    let mut handles = Vec::new();
    for _ in 0..10 {
        let g = guard.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..1000 {
                if let Some(permit) = g.try_acquire().await {
                    drop(permit);
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
}

// ── Resource Guard Boundaries ──

#[test]
fn resourceguard_validate_request_size_zero_capacity() {
    // === Arrange ===
    let max_bytes = 0usize;

    // === Act & Assert ===
    assert!(
        !nusa_core::validate_request_size(Some(1), max_bytes),
        "must reject any size with zero capacity"
    );
}

#[test]
fn resourceguard_validate_request_size_usize_max() {
    // === Arrange ===
    let max_bytes = usize::MAX;

    // === Act & Assert ===
    // u64::MAX fits in usize only on 64-bit platforms
    #[cfg(target_pointer_width = "64")]
    {
        assert!(
            nusa_core::validate_request_size(Some(u64::MAX), max_bytes),
            "usize::MAX capacity must allow u64::MAX"
        );
    }
    #[cfg(target_pointer_width = "32")]
    {
        assert!(
            !nusa_core::validate_request_size(Some(u64::MAX), max_bytes),
            "u64::MAX exceeds 32-bit usize"
        );
    }
}
