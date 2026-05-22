//! Stress test suite for gateway.
//!
//! rust-test §Stress Test Matrix
//! Covers throughput, load ramp, spike, thundering herd, backpressure, degradation.

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::Semaphore;
use tower::ServiceExt;

use nusa_core::{
    BackpressureGuard, PhpEngine, PhpResponse, RequestContext, ResourceGuard, TaskManager,
    TenantRateLimiter, TenantRegistry,
};
use nusa_gateway::app;
use nusa_gateway::circuit_breaker::CircuitBreaker;
use nusa_gateway::health::HealthState;
use nusa_gateway::sse::SseManager;
use nusa_gateway::static_files::StaticFileHandler;
use nusa_gateway::tenant_circuit_breaker::TenantCircuitBreakers;
use nusa_gateway::websocket::WsManager;
use nusa_octane_worker::state_reset::StateResetOrchestrator;
use nusa_plugin_api::PluginRegistry;
use nusa_telemetry::metrics::NusaMetrics;
use parking_lot::Mutex;

static PROMETHEUS_HANDLE: OnceLock<Arc<metrics_exporter_prometheus::PrometheusHandle>> =
    OnceLock::new();

fn get_prometheus_handle() -> Arc<metrics_exporter_prometheus::PrometheusHandle> {
    PROMETHEUS_HANDLE
        .get_or_init(|| {
            Arc::new(
                metrics_exporter_prometheus::PrometheusBuilder::new()
                    .install_recorder()
                    .expect("prometheus recorder"),
            )
        })
        .clone()
}

struct FastMockEngine;

#[async_trait::async_trait]
impl PhpEngine for FastMockEngine {
    async fn execute(&self, _ctx: RequestContext) -> nusa_core::Result<PhpResponse> {
        Ok(PhpResponse {
            status: 200,
            headers: Default::default(),
            body: bytes::Bytes::from("ok"),
        })
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &["fast"]
    }
    async fn shutdown(&self) {}
}

fn build_test_app() -> Router {
    let prometheus_handle = get_prometheus_handle();
    app(
        Arc::new(FastMockEngine),
        Arc::new(PluginRegistry::new()),
        Arc::new(CircuitBreaker::new(100, Duration::from_secs(1))),
        Arc::new(HealthState::new()),
        Arc::new(BackpressureGuard::new(50)),
        ResourceGuard {
            max_request_bytes: 1024 * 1024,
            request_timeout_ms: 5000,
            max_concurrent: 50,
        },
        Arc::new(TenantRegistry::new()),
        Arc::new(TaskManager::new()),
        Arc::new(TenantRateLimiter::new(10000, 500)),
        Arc::new(TenantCircuitBreakers::new(50, Duration::from_secs(10))),
        Arc::new(WsManager::new()),
        Arc::new(SseManager::new()),
        Arc::new(StaticFileHandler::new("/app/public".into())),
        Arc::new(NusaMetrics::init()),
        prometheus_handle.clone(),
        Arc::new(Mutex::new(None)),
        Arc::new(Mutex::new({
            let mut r = StateResetOrchestrator::new(128);
            r.initialize();
            r
        })),
    )
}

fn make_request() -> Request<Body> {
    Request::builder()
        .uri("/index.php")
        .method("GET")
        .body(Body::empty())
        .unwrap()
}

// ── Throughput Tests ──

#[tokio::test]
async fn stress_throughput_100_requests() {
    let app = build_test_app();
    let start = std::time::Instant::now();

    let mut handles = Vec::new();
    for _ in 0..100 {
        let app = app.clone();
        handles.push(tokio::spawn(async move {
            let resp = app.oneshot(make_request()).await.unwrap();
            resp.status()
        }));
    }

    let mut ok_count = 0;
    for h in handles {
        if h.await.unwrap() == StatusCode::OK {
            ok_count += 1;
        }
    }

    let elapsed = start.elapsed();
    // All 100 requests should succeed
    assert_eq!(ok_count, 100, "all 100 requests should succeed");
    // Should complete within 10 seconds
    assert!(
        elapsed < Duration::from_secs(10),
        "100 requests took {:?}",
        elapsed
    );
}

#[tokio::test]
async fn stress_throughput_500_requests() {
    let app = build_test_app();
    let start = std::time::Instant::now();

    // Use semaphore to limit concurrency
    let sem = Arc::new(Semaphore::new(20));
    let mut handles = Vec::new();

    for _ in 0..500 {
        let app = app.clone();
        let sem = sem.clone();
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let resp = app.oneshot(make_request()).await.unwrap();
            resp.status()
        }));
    }

    let mut ok_count = 0;
    for h in handles {
        if h.await.unwrap() == StatusCode::OK {
            ok_count += 1;
        }
    }

    let elapsed = start.elapsed();
    assert_eq!(ok_count, 500, "all 500 requests should succeed");
    assert!(
        elapsed < Duration::from_secs(30),
        "500 requests took {:?}",
        elapsed
    );
}

// ── Spike Load Tests ──

#[tokio::test]
async fn stress_spike_2x_baseline() {
    let app = build_test_app();

    // Baseline: 10 requests
    let baseline_start = std::time::Instant::now();
    for _ in 0..10 {
        app.clone().oneshot(make_request()).await.unwrap();
    }
    let baseline_time = baseline_start.elapsed();

    // Spike: 20 requests concurrently
    let spike_start = std::time::Instant::now();
    let mut handles = Vec::new();
    for _ in 0..20 {
        let app = app.clone();
        handles.push(tokio::spawn(async move {
            app.oneshot(make_request()).await.unwrap().status()
        }));
    }

    let mut ok = 0;
    for h in handles {
        if h.await.unwrap() == StatusCode::OK {
            ok += 1;
        }
    }
    let spike_time = spike_start.elapsed();

    assert_eq!(ok, 20, "all spike requests should succeed");
    // Spike should not take more than 3x baseline time
    assert!(
        spike_time < baseline_time * 3,
        "spike took {:?} vs baseline {:?}",
        spike_time,
        baseline_time
    );
}

#[tokio::test]
async fn stress_spike_repeated_no_accumulation() {
    let app = build_test_app();

    // Run 5 spike bursts
    for burst in 0..5 {
        let mut handles = Vec::new();
        for _ in 0..20 {
            let app = app.clone();
            handles.push(tokio::spawn(async move {
                app.oneshot(make_request()).await.unwrap().status()
            }));
        }

        let mut ok = 0;
        for h in handles {
            if h.await.unwrap() == StatusCode::OK {
                ok += 1;
            }
        }
        assert_eq!(ok, 20, "burst {} all requests should succeed", burst);
    }
}

// ── Backpressure Tests ──

#[tokio::test]
async fn stress_backpressure_rejects_at_capacity() {
    let guard = BackpressureGuard::new(2);

    // Acquire all permits
    let p1 = guard.try_acquire().await;
    let p2 = guard.try_acquire().await;
    assert!(p1.is_some());
    assert!(p2.is_some());

    // Next request should be rejected
    let p3 = guard.try_acquire().await;
    assert!(p3.is_none(), "should reject when at capacity");

    // Drop one permit
    drop(p1);

    // Now should allow again
    let p4 = guard.try_acquire().await;
    assert!(p4.is_some(), "should allow after permit released");
}

#[tokio::test]
async fn stress_backpressure_concurrent_acquire_release() {
    let guard = Arc::new(BackpressureGuard::new(5));
    let mut handles = Vec::new();

    for _ in 0..20 {
        let g = guard.clone();
        handles.push(tokio::spawn(async move {
            let permit = g.try_acquire().await;
            tokio::time::sleep(Duration::from_millis(10)).await;
            permit.is_some()
        }));
    }

    let mut acquired = 0;
    for h in handles {
        if h.await.unwrap() {
            acquired += 1;
        }
    }

    // At least some should have been acquired (concurrency means not all 20 will get permits at once)
    assert!(acquired > 0, "at least some requests should acquire");
    assert!(acquired <= 20, "no more than total requests");
}

// ── Thundering Herd Test ──

#[tokio::test]
async fn stress_thundering_herd_simultaneous_requests() {
    let app = build_test_app();
    let barrier = Arc::new(tokio::sync::Barrier::new(50));

    let mut handles = Vec::new();
    for _ in 0..50 {
        let app = app.clone();
        let barrier = barrier.clone();
        handles.push(tokio::spawn(async move {
            barrier.wait().await; // All start at the same time
            app.oneshot(make_request()).await.unwrap().status()
        }));
    }

    let mut ok = 0;
    for h in handles {
        if h.await.unwrap() == StatusCode::OK {
            ok += 1;
        }
    }

    // All 50 should succeed (server should handle thundering herd)
    assert_eq!(ok, 50, "all thundering herd requests should succeed");
}

// ── Degradation & Recovery Tests ──

#[tokio::test]
async fn stress_circuit_breaker_degradation_and_recovery() {
    let cb = Arc::new(CircuitBreaker::new(3, Duration::from_millis(100)));

    // Phase 1: Normal operation
    assert!(cb.allow_request(), "circuit should be closed");
    cb.record_success();

    // Phase 2: Trip the circuit
    for _ in 0..3 {
        cb.record_failure();
    }
    assert!(!cb.allow_request(), "circuit should be open");

    // Phase 3: Wait for recovery
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Phase 4: Should recover (half-open)
    assert!(cb.allow_request(), "circuit should be half-open");
    cb.record_success();
    assert!(cb.allow_request(), "circuit should be closed again");
}

#[tokio::test]
async fn stress_circuit_breaker_rapid_failure_recovery_cycle() {
    let cb = CircuitBreaker::new(2, Duration::from_millis(50));

    // Rapid open/close cycles
    for cycle in 0..10 {
        cb.record_failure();
        cb.record_failure();
        assert!(!cb.allow_request(), "cycle {} should be open", cycle);

        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(cb.allow_request(), "cycle {} should be half-open", cycle);
        cb.record_success();
    }
}
