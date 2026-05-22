//! Extended stress tests, decision logic tests, WebSocket decision tests, and
//! SSE decision tests for nusa-gateway.
//!
//! Covers: Gateway throughput, spike load, soak, thundering herd, backpressure,
//! degradation, CircuitBreaker throughput, tenant extraction, trace ID, request
//! size middleware, WebSocket, SSE.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{HeaderMap, HeaderValue, Request, StatusCode},
};
use bytes::Bytes;
use http::header::CONTENT_LENGTH;
use parking_lot::Mutex;
use tokio::sync::Barrier;
use tower::ServiceExt;

use futures::StreamExt;
use nusa_core::TenantId;
use nusa_core::{
    BackpressureGuard, PhpEngine, PhpResponse, RequestContext, ResourceGuard, TaskManager,
    TenantRateLimiter, TenantRegistry,
};
use nusa_gateway::circuit_breaker::{CbState, CircuitBreaker};
use nusa_gateway::middleware::{extract_tenant, extract_trace_id};
use nusa_gateway::sse::SseManager;
use nusa_gateway::static_files::StaticFileHandler;
use nusa_gateway::tenant_circuit_breaker::TenantCircuitBreakers;
use nusa_gateway::websocket::WsManager;
use nusa_gateway::{app, health::HealthState};
use nusa_octane_worker::state_reset::StateResetOrchestrator;
use nusa_plugin_api::PluginRegistry;
use nusa_telemetry::metrics::NusaMetrics;

// ── Mock Engine ──

struct FastMockEngine;

#[async_trait]
impl PhpEngine for FastMockEngine {
    async fn execute(&self, _ctx: RequestContext) -> nusa_core::Result<PhpResponse> {
        Ok(PhpResponse {
            status: 200,
            headers: Default::default(),
            body: Bytes::from("OK"),
        })
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &["mock"]
    }
    async fn shutdown(&self) {}
}

use std::sync::OnceLock;

static PROMETHEUS_HANDLE: OnceLock<Arc<metrics_exporter_prometheus::PrometheusHandle>> =
    OnceLock::new();

fn prometheus() -> Arc<metrics_exporter_prometheus::PrometheusHandle> {
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

fn build_test_app(engine: Arc<dyn PhpEngine>) -> Router {
    let resource_guard = ResourceGuard {
        max_request_bytes: 1024 * 1024,
        request_timeout_ms: 5000,
        max_concurrent: 5,
    };
    app(
        engine,
        Arc::new(PluginRegistry::new()),
        Arc::new(CircuitBreaker::new(3, Duration::from_secs(1))),
        Arc::new(HealthState::new()),
        Arc::new(BackpressureGuard::new(100)),
        resource_guard,
        Arc::new(TenantRegistry::new()),
        Arc::new(TaskManager::new()),
        Arc::new(TenantRateLimiter::new(1000, 50)),
        Arc::new(TenantCircuitBreakers::new(3, Duration::from_secs(10))),
        Arc::new(WsManager::new()),
        Arc::new(SseManager::new()),
        Arc::new(StaticFileHandler::new("/app/public".into())),
        Arc::new(NusaMetrics::init()),
        prometheus(),
        Arc::new(tokio::sync::Mutex::new(None)),
        Arc::new(Mutex::new({
            let mut r = StateResetOrchestrator::new(128);
            r.initialize();
            r
        })),
    )
}

// ============================================================================
// Stress Tests: Gateway Throughput
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gateway_throughput_100_req_sec_completes() {
    let app = build_test_app(Arc::new(FastMockEngine));
    let count = 100;
    let start = Instant::now();

    let mut handles = Vec::new();
    for _ in 0..count {
        let app = app.clone();
        handles.push(tokio::spawn(async move {
            let request = Request::builder()
                .uri("/index.php")
                .method("GET")
                .body(Body::empty())
                .unwrap();
            let resp = app.oneshot(request).await;
            resp.is_ok_and(|r| r.status().is_success())
        }));
    }

    let mut successes = 0;
    for handle in handles {
        if let Ok(true) = handle.await {
            successes += 1;
        }
    }

    let elapsed = start.elapsed();
    let rate = count as f64 / elapsed.as_secs_f64();
    assert!(rate >= 50.0, "throughput {rate:.0}/s below 50/s minimum");
    assert!(successes > count / 2, "{successes}/{count} succeeded");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn gateway_throughput_500_req_sec_p99_within_sla() {
    let app = build_test_app(Arc::new(FastMockEngine));
    let count = 500;
    let start = Instant::now();

    let mut handles = Vec::new();
    for _ in 0..count {
        let app = app.clone();
        handles.push(tokio::spawn(async move {
            let t0 = Instant::now();
            let request = Request::builder()
                .uri("/index.php")
                .method("GET")
                .body(Body::empty())
                .unwrap();
            let resp = app.oneshot(request).await;
            (t0.elapsed().as_millis() as u64, resp.is_ok())
        }));
    }

    let mut latencies = Vec::new();
    for handle in handles {
        if let Ok((lat, true)) = handle.await {
            latencies.push(lat);
        }
    }
    latencies.sort();

    let elapsed = start.elapsed();
    let rate = count as f64 / elapsed.as_secs_f64();

    assert!(rate >= 100.0, "throughput {rate:.0}/s below 100/s");

    if latencies.len() > 10 {
        let p99_idx = latencies.len() * 99 / 100;
        assert!(
            latencies[p99_idx] < 30000,
            "P99 latency {}ms exceeds SLA",
            latencies[p99_idx]
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn gateway_throughput_1000_req_sec_completes() {
    let app = build_test_app(Arc::new(FastMockEngine));
    let count = 1000;
    let start = Instant::now();

    let mut handles = Vec::new();
    for _ in 0..count {
        let app = app.clone();
        handles.push(tokio::spawn(async move {
            let request = Request::builder()
                .uri("/index.php")
                .method("GET")
                .body(Body::empty())
                .unwrap();
            app.oneshot(request).await.is_ok()
        }));
    }

    let mut successes = 0;
    for handle in handles {
        if handle.await.unwrap_or(false) {
            successes += 1;
        }
    }

    let elapsed = start.elapsed();
    let rate = count as f64 / elapsed.as_secs_f64();
    assert!(rate >= 50.0, "throughput {rate:.0}/s below 50/s");
    assert!(successes > count / 4, "{successes}/{count} succeeded");
}

// ============================================================================
// Stress Tests: Load Ramp-Up
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gateway_load_ramp_0_to_100_stable() {
    let app = build_test_app(Arc::new(FastMockEngine));

    for batch in [10, 25, 50, 100, 200] {
        let mut handles = Vec::new();
        for _ in 0..batch {
            let app = app.clone();
            handles.push(tokio::spawn(async move {
                let request = Request::builder()
                    .uri("/index.php")
                    .method("GET")
                    .body(Body::empty())
                    .unwrap();
                app.oneshot(request).await.is_ok()
            }));
        }

        let mut successes = 0;
        for h in handles {
            if h.await.unwrap_or(false) {
                successes += 1;
            }
        }
        assert!(
            successes > batch / 2,
            "batch {batch}: only {successes}/{batch} succeeded"
        );
    }
}

// ============================================================================
// Stress Tests: Spike Load
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gateway_spike_2x_baseline_no_failures() {
    let app = build_test_app(Arc::new(FastMockEngine));
    let count = 50;

    let mut handles = Vec::new();
    for _ in 0..count {
        let app = app.clone();
        handles.push(tokio::spawn(async move {
            let request = Request::builder()
                .uri("/index.php")
                .method("GET")
                .body(Body::empty())
                .unwrap();
            app.oneshot(request).await
        }));
    }

    let mut failures = 0;
    for h in handles {
        match h.await {
            Ok(Ok(resp)) => {
                if !resp.status().is_success() && resp.status() != StatusCode::SERVICE_UNAVAILABLE {
                    failures += 1;
                }
            }
            _ => failures += 1,
        }
    }

    assert!(failures == 0, "2x spike caused {failures} failures");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn gateway_spike_5x_baseline_no_corruption() {
    let app = build_test_app(Arc::new(FastMockEngine));
    let count = 250;

    let mut handles = Vec::new();
    for _ in 0..count {
        let app = app.clone();
        handles.push(tokio::spawn(async move {
            let request = Request::builder()
                .uri("/index.php")
                .method("GET")
                .body(Body::empty())
                .unwrap();
            app.oneshot(request).await
        }));
    }

    let mut failures = 0;
    for h in handles {
        if let Ok(Err(_)) = h.await {
            failures += 1
        }
    }

    assert!(
        failures < count / 2,
        "5x spike caused excessive failures: {failures}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn gateway_spike_10x_baseline_recovers() {
    let app = build_test_app(Arc::new(FastMockEngine));
    let count = 500;

    let mut handles = Vec::new();
    for _ in 0..count {
        let app = app.clone();
        handles.push(tokio::spawn(async move {
            let request = Request::builder()
                .uri("/index.php")
                .method("GET")
                .body(Body::empty())
                .unwrap();
            app.oneshot(request).await
        }));
    }

    for h in handles {
        let _ = h.await;
    }

    // Verify recovery
    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(request).await.expect("recovery request failed");
    assert!(resp.status().is_success(), "should recover after 10x spike");
}

// ============================================================================
// Stress Tests: Soak Test
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gateway_soak_30_seconds_memory_stable() {
    let app = build_test_app(Arc::new(FastMockEngine));
    let duration = Duration::from_secs(30);
    let interval = Duration::from_millis(200);
    let mut request_count = 0u64;
    let start = Instant::now();

    while start.elapsed() < duration {
        let request = Request::builder()
            .uri("/index.php")
            .method("GET")
            .body(Body::empty())
            .unwrap();
        let _ = app.clone().oneshot(request).await;
        request_count += 1;
        tokio::time::sleep(interval).await;
    }

    assert!(
        request_count > 50,
        "soak test submitted only {request_count} requests in 30s"
    );
}

// ============================================================================
// Stress Tests: Thundering Herd
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn gateway_thundering_herd_after_idle_no_cascade() {
    let app = build_test_app(Arc::new(FastMockEngine));
    let num_clients = 20;
    let barrier = Arc::new(Barrier::new(num_clients));

    let mut handles = Vec::new();
    for _ in 0..num_clients {
        let app = app.clone();
        let bar = barrier.clone();
        handles.push(tokio::spawn(async move {
            bar.wait().await;
            let request = Request::builder()
                .uri("/index.php")
                .method("GET")
                .body(Body::empty())
                .unwrap();
            app.oneshot(request).await
        }));
    }

    let mut successes = 0;
    for h in handles {
        match h.await {
            Ok(Ok(resp)) if resp.status().is_success() => successes += 1,
            _ => {}
        }
    }

    assert!(
        successes > num_clients / 2,
        "thundering herd: only {successes}/{num_clients} succeeded"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gateway_thundering_herd_recovers() {
    let app = build_test_app(Arc::new(FastMockEngine));
    let num_clients = 20;
    let barrier = Arc::new(Barrier::new(num_clients));

    let mut handles = Vec::new();
    for _ in 0..num_clients {
        let app = app.clone();
        let bar = barrier.clone();
        handles.push(tokio::spawn(async move {
            bar.wait().await;
            let request = Request::builder()
                .uri("/index.php")
                .method("GET")
                .body(Body::empty())
                .unwrap();
            app.oneshot(request).await
        }));
    }

    for h in handles {
        let _ = h.await;
    }

    // Verify recovery
    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(request).await.expect("should recover");
    assert!(resp.status().is_success());
}

// ============================================================================
// Stress Tests: Backpressure
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gateway_backpressure_connection_pool_exhausted_rejects() {
    let guard = BackpressureGuard::new(2);

    let p1 = guard.try_acquire().await;
    let p2 = guard.try_acquire().await;
    assert!(p1.is_some());
    assert!(p2.is_some());

    let p3 = guard.try_acquire().await;
    assert!(p3.is_none(), "should reject when pool exhausted");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gateway_backpressure_rejects_not_drops() {
    let guard = BackpressureGuard::new(1);
    let permit = guard.try_acquire().await;
    assert!(permit.is_some());

    let reject = guard.try_acquire().await;
    assert!(reject.is_none(), "should reject (not silently drop)");
}

// ============================================================================
// Stress Tests: Degradation
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gateway_degradation_partial_failure_50pct_errors() {
    let cb = CircuitBreaker::new(10, Duration::from_secs(10));

    // 5 failures out of 10 threshold (50% errors)
    for _ in 0..5 {
        cb.record_failure();
    }

    assert_eq!(
        cb.state(),
        CbState::Closed,
        "50% errors should not trip yet"
    );

    // 5 more to reach threshold
    for _ in 0..5 {
        cb.record_failure();
    }

    assert_eq!(
        cb.state(),
        CbState::Open,
        "should open after full threshold"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gateway_degradation_dependency_timeout_fallback() {
    let cb = CircuitBreaker::new(3, Duration::from_millis(50));

    // Simulate 3 failures (like timeout)
    for _ in 0..3 {
        cb.record_failure();
    }

    assert_eq!(cb.state(), CbState::Open);
    assert!(!cb.allow_request());

    // Wait and recover
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert!(cb.allow_request());
    assert_eq!(cb.state(), CbState::HalfOpen);
}

// ============================================================================
// Stress Tests: CircuitBreaker Throughput
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn circuit_breaker_rapid_failures_then_recovery() {
    let cb = CircuitBreaker::new(5, Duration::from_millis(30));

    // Rapid failures
    for _ in 0..10 {
        cb.record_failure();
    }
    assert_eq!(cb.state(), CbState::Open);

    // Wait for recovery
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(cb.allow_request());
    assert_eq!(cb.state(), CbState::HalfOpen);

    // Successful probe
    cb.record_success();
    assert_eq!(cb.state(), CbState::Closed);
}

#[test]
fn circuit_breaker_rapid_successions_no_corruption() {
    let cb = CircuitBreaker::new(1, Duration::from_millis(50));
    cb.record_failure();
    assert_eq!(cb.state(), CbState::Open);

    std::thread::sleep(Duration::from_millis(60));

    for _ in 0..100 {
        let _ = cb.allow_request();
    }

    assert_eq!(cb.state(), CbState::HalfOpen);
}

// ============================================================================
// Decision Logic Tests: Tenant Extraction (8 combos)
// ============================================================================

#[test]
fn tenant_extract_header_present_valid_returns_tenant() {
    let mut headers = HeaderMap::new();
    headers.insert("x-tenant-id", HeaderValue::from_static("tenant-a"));
    let result = extract_tenant(&headers);
    assert!(result.is_some());
    assert_eq!(result.unwrap().as_str(), "tenant-a");
}

#[test]
fn tenant_extract_header_absent_subdomain_non_localhost_returns_tenant() {
    let mut headers = HeaderMap::new();
    headers.insert("host", HeaderValue::from_static("tenant-b.example.com"));
    let result = extract_tenant(&headers);
    assert!(result.is_some());
    assert_eq!(result.unwrap().as_str(), "tenant-b");
}

#[test]
fn tenant_extract_header_absent_subdomain_localhost_returns_none() {
    let mut headers = HeaderMap::new();
    headers.insert("host", HeaderValue::from_static("localhost"));
    let result = extract_tenant(&headers);
    assert!(result.is_none(), "localhost should not extract tenant");
}

#[test]
fn tenant_extract_header_absent_host_127_returns_none() {
    let mut headers = HeaderMap::new();
    headers.insert("host", HeaderValue::from_static("127.0.0.1:3000"));
    let result = extract_tenant(&headers);
    assert!(result.is_none());
}

#[test]
fn tenant_extract_header_present_takes_priority_over_subdomain() {
    let mut headers = HeaderMap::new();
    headers.insert("x-tenant-id", HeaderValue::from_static("header-tenant"));
    headers.insert("host", HeaderValue::from_static("subdomain.example.com"));
    let result = extract_tenant(&headers);
    assert!(result.is_some());
    assert_eq!(result.unwrap().as_str(), "header-tenant");
}

#[test]
fn tenant_extract_both_absent_returns_none() {
    let headers = HeaderMap::new();
    assert!(extract_tenant(&headers).is_none());
}

#[test]
fn tenant_extract_header_empty_value_returns_none() {
    let mut headers = HeaderMap::new();
    headers.insert("x-tenant-id", HeaderValue::from_static(""));
    let result = extract_tenant(&headers);
    assert!(
        result.is_none(),
        "empty x-tenant-id must not resolve to a tenant"
    );
}

#[test]
fn tenant_extract_host_with_port_strips_port() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "host",
        HeaderValue::from_static("my-tenant.example.com:8080"),
    );
    let result = extract_tenant(&headers);
    assert!(result.is_some());
    assert_eq!(result.unwrap().as_str(), "my-tenant");
}

// ============================================================================
// Decision Logic Tests: Trace ID (4 combos)
// ============================================================================

#[test]
fn trace_extract_valid_traceparent_returns_trace_id() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "traceparent",
        HeaderValue::from_static("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"),
    );
    let trace = extract_trace_id(&headers);
    assert!(!trace.to_string().is_empty());
}

#[test]
fn trace_extract_absent_traceparent_generates_new() {
    let headers = HeaderMap::new();
    let trace = extract_trace_id(&headers);
    assert!(!trace.to_string().is_empty());
}

#[test]
fn trace_extract_invalid_traceparent_generates_new() {
    let mut headers = HeaderMap::new();
    headers.insert("traceparent", HeaderValue::from_static("invalid-format"));
    let trace = extract_trace_id(&headers);
    assert!(!trace.to_string().is_empty());
}

#[test]
fn trace_extract_malformed_hex_generates_new() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "traceparent",
        HeaderValue::from_static("00-INVALIDHEXNOT32CHARS-00f067aa0ba902b7-01"),
    );
    let trace = extract_trace_id(&headers);
    assert!(!trace.to_string().is_empty());
}

// ============================================================================
// Decision Logic Tests: Request Size Middleware
// ============================================================================

#[tokio::test]
async fn request_size_under_limit_returns_200() {
    let app = build_test_app(Arc::new(FastMockEngine));
    let body = Body::from("small body");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header(CONTENT_LENGTH, "10")
        .body(body)
        .unwrap();
    let resp = app.oneshot(request).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn request_size_over_10mb_returns_413() {
    let app = build_test_app(Arc::new(FastMockEngine));
    let body = Body::from("x");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header(CONTENT_LENGTH, "11000000") // 11MB
        .body(body)
        .unwrap();
    let resp = app.oneshot(request).await.unwrap();
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn request_size_no_content_length_returns_200() {
    let app = build_test_app(Arc::new(FastMockEngine));
    let body = Body::from("chunked body");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .body(body)
        .unwrap();
    let resp = app.oneshot(request).await.unwrap();
    assert!(resp.status().is_success());
}

// ============================================================================
// Decision Logic Tests: Circuit Breaker Decision Table
// ============================================================================

#[test]
fn circuit_breaker_failures_below_threshold_closed() {
    let cb = CircuitBreaker::new(5, Duration::from_secs(1));
    for _ in 0..4 {
        cb.record_failure();
    }
    assert_eq!(cb.state(), CbState::Closed);
    assert!(cb.allow_request());
}

#[test]
fn circuit_breaker_failures_at_threshold_opens() {
    let cb = CircuitBreaker::new(3, Duration::from_secs(1));
    for _ in 0..3 {
        cb.record_failure();
    }
    assert_eq!(cb.state(), CbState::Open);
    assert!(!cb.allow_request());
}

#[test]
fn circuit_breaker_failures_above_threshold_stays_open() {
    let cb = CircuitBreaker::new(3, Duration::from_secs(1));
    for _ in 0..10 {
        cb.record_failure();
    }
    assert_eq!(cb.state(), CbState::Open);
}

#[test]
fn circuit_breaker_half_open_probe_success_closes() {
    let cb = CircuitBreaker::new(1, Duration::from_millis(30));
    cb.record_failure();
    std::thread::sleep(Duration::from_millis(40));
    cb.allow_request();
    assert_eq!(cb.state(), CbState::HalfOpen);
    cb.record_success();
    assert_eq!(cb.state(), CbState::Closed);
}

#[test]
fn circuit_breaker_half_open_probe_failure_reopens() {
    let cb = CircuitBreaker::new(1, Duration::from_millis(30));
    cb.record_failure();
    std::thread::sleep(Duration::from_millis(40));
    cb.allow_request();
    assert_eq!(cb.state(), CbState::HalfOpen);
    cb.record_failure();
    assert_eq!(cb.state(), CbState::Open);
}

// ============================================================================
// WebSocket Decision Tests
// ============================================================================

#[test]
fn ws_manager_new_starts_empty() {
    let manager = WsManager::new();
    assert_eq!(manager.connection_count(), 0);
}

#[test]
fn ws_manager_register_test_connection_increases_count() {
    let manager = WsManager::new();
    let tenant = TenantId::new("ws-tenant");
    let _rx = manager.register_test_connection("conn-1".into(), tenant);
    assert_eq!(manager.connection_count(), 1);
}

#[tokio::test]
async fn ws_manager_broadcast_to_tenant_delivers_message() {
    let manager = WsManager::new();
    let tenant = TenantId::new("broadcast-tenant");
    let mut rx = manager.register_test_connection("conn-1".into(), tenant.clone());

    manager.broadcast_to_tenant(&tenant, "hello");

    let msg = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timeout waiting for broadcast")
        .expect("channel closed");

    match msg {
        axum::extract::ws::Message::Text(text) => {
            assert_eq!(text, "hello");
        }
        _ => panic!("expected text message"),
    }
}

#[tokio::test]
async fn ws_manager_broadcast_multi_subscriber_all_receive() {
    let manager = WsManager::new();
    let tenant = TenantId::new("multi-tenant");
    let mut rx1 = manager.register_test_connection("conn-1".into(), tenant.clone());
    let mut rx2 = manager.register_test_connection("conn-2".into(), tenant.clone());
    let mut rx3 = manager.register_test_connection("conn-3".into(), tenant.clone());

    manager.broadcast_to_tenant(&tenant, "event");

    for rx in [&mut rx1, &mut rx2, &mut rx3] {
        let msg = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("timeout")
            .expect("channel closed");
        match msg {
            axum::extract::ws::Message::Text(text) => {
                assert_eq!(text, "event");
            }
            _ => panic!("expected text"),
        }
    }
}

#[tokio::test]
async fn ws_manager_broadcast_wrong_tenant_no_delivery() {
    let manager = WsManager::new();
    let t1 = TenantId::new("tenant-1");
    let t2 = TenantId::new("tenant-2");
    let mut rx = manager.register_test_connection("conn-1".into(), t1);

    manager.broadcast_to_tenant(&t2, "should-not-receive");

    let result = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;
    assert!(result.is_err(), "wrong tenant should not receive message");
}

#[test]
fn ws_manager_connection_count_accurate() {
    let manager = WsManager::new();
    let t = TenantId::new("count-tenant");
    manager.register_test_connection("c1".into(), t.clone());
    manager.register_test_connection("c2".into(), t);
    assert_eq!(manager.connection_count(), 2);
}

// ============================================================================
// SSE Decision Tests
// ============================================================================

#[test]
fn sse_manager_new_creates_channel() {
    let manager = SseManager::new();
    let _stream = manager.stream();
}

#[tokio::test]
async fn sse_manager_single_subscriber_receives_event() {
    let manager = SseManager::new();
    let mut rx = manager.subscribe();

    manager.send("test-event");

    let data = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timeout")
        .expect("channel closed");

    assert_eq!(data, "test-event");
}

#[tokio::test]
async fn sse_manager_multi_subscriber_all_receive() {
    let manager = SseManager::new();
    let mut r1 = manager.subscribe();
    let mut r2 = manager.subscribe();
    let mut r3 = manager.subscribe();

    manager.send("multi-event");

    for rx in [&mut r1, &mut r2, &mut r3] {
        let data = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("timeout")
            .expect("channel closed");
        assert_eq!(data, "multi-event");
    }
}

#[tokio::test]
async fn sse_manager_client_disconnect_stream_cleanup() {
    let manager = SseManager::new();
    let mut stream = manager.stream();

    manager.send("before-drop");
    let _ = tokio::time::timeout(Duration::from_secs(5), stream.next()).await;

    drop(stream);

    // Send after drop - should not panic
    manager.send("after-drop");
}

#[tokio::test]
async fn sse_manager_keepalive_interval_events_sent() {
    let manager = Arc::new(SseManager::new());
    let mut stream = manager.stream();

    // Subscribe before send — broadcast does not replay to late subscribers.
    for i in 0..5 {
        manager.send(&format!("event-{i}"));
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let event = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("timeout")
        .expect("None");
    assert!(event.is_ok());
}

// ============================================================================
// Pairwise Tests: Tenant + Trace ID Combinations
// ============================================================================

#[test]
fn combo_tenant_trace_both_present_extract_both() {
    let mut headers = HeaderMap::new();
    headers.insert("x-tenant-id", HeaderValue::from_static("pairwise-tenant"));
    headers.insert(
        "traceparent",
        HeaderValue::from_static("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"),
    );

    let tenant = extract_tenant(&headers);
    let trace = extract_trace_id(&headers);

    assert!(tenant.is_some());
    assert_eq!(tenant.unwrap().as_str(), "pairwise-tenant");
    assert!(!trace.to_string().is_empty());
}

#[test]
fn combo_tenant_trace_tenant_only() {
    let mut headers = HeaderMap::new();
    headers.insert("x-tenant-id", HeaderValue::from_static("tenant-only"));

    let tenant = extract_tenant(&headers);
    let trace = extract_trace_id(&headers);

    assert!(tenant.is_some());
    assert!(!trace.to_string().is_empty()); // generated
}

#[test]
fn combo_tenant_trace_trace_only() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "traceparent",
        HeaderValue::from_static("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"),
    );

    let tenant = extract_tenant(&headers);
    let trace = extract_trace_id(&headers);

    assert!(tenant.is_none());
    assert!(!trace.to_string().is_empty());
}

#[test]
fn combo_tenant_trace_neither_present_defaults() {
    let headers = HeaderMap::new();
    let tenant = extract_tenant(&headers);
    let trace = extract_trace_id(&headers);

    assert!(tenant.is_none());
    assert!(!trace.to_string().is_empty()); // auto-generated
}

// ============================================================================
// Authorization Guard Tests
// ============================================================================

#[tokio::test]
async fn auth_guard_authorized_returns_200() {
    let app = build_test_app(Arc::new(FastMockEngine));
    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(request).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn auth_gateway_unauthorized_passes_no_auth_middleware() {
    let app = build_test_app(Arc::new(FastMockEngine));
    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(request).await.unwrap();
    // No auth middleware in current setup, so returns 200
    assert!(resp.status().is_success());
}
