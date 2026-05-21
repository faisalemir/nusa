//! Detailed integration tests for nusa-gateway
//!
//! Skills applied:
//! - `domain-web`: HTTP request handling, middleware chain
//! - `m07-concurrency`: Arc<dyn PhpEngine> shared across handlers
//! - `m13-domain-error`: Error → HTTP status mapping
//! - `m06-error-handling`: Result propagation, status mapping

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use bytes::Bytes;
use parking_lot::Mutex;
use tower::ServiceExt;

use nusa_core::{
    BackpressureGuard, PhpEngine, PhpResponse, RequestContext, ResourceGuard, TaskManager,
    TenantRateLimiter, TenantRegistry,
};
use nusa_gateway::circuit_breaker::CircuitBreaker;
use nusa_gateway::sse::SseManager;
use nusa_gateway::static_files::StaticFileHandler;
use nusa_gateway::tenant_circuit_breaker::TenantCircuitBreakers;
use nusa_gateway::websocket::WsManager;
use nusa_gateway::{app, health::HealthState};
use nusa_octane_worker::state_reset::StateResetOrchestrator;
use nusa_plugin_api::PluginRegistry;
use nusa_telemetry::metrics::NusaMetrics;

// ── Mock Engines ──

struct MockEngine;

#[async_trait]
impl PhpEngine for MockEngine {
    async fn execute(&self, _ctx: RequestContext) -> nusa_core::Result<PhpResponse> {
        Ok(PhpResponse {
            status: 200,
            headers: Default::default(),
            body: Bytes::from("Mock PHP Response"),
        })
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &["mock"]
    }
    async fn shutdown(&self) {}
}

struct FailingMockEngine;

#[async_trait]
impl PhpEngine for FailingMockEngine {
    async fn execute(&self, _ctx: RequestContext) -> nusa_core::Result<PhpResponse> {
        Err(nusa_core::EngineError::PhpFatal("Test failure".into()))
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &["mock"]
    }
    async fn shutdown(&self) {}
}

// ── Test Helpers ──

use std::sync::OnceLock;

static PROMETHEUS_HANDLE: OnceLock<Arc<metrics_exporter_prometheus::PrometheusHandle>> = OnceLock::new();

fn get_prometheus_handle() -> Arc<metrics_exporter_prometheus::PrometheusHandle> {
    PROMETHEUS_HANDLE
        .get_or_init(|| {
            // First call succeeds, subsequent calls return the same handle
            // This avoids the "FailedToSetGlobalRecorder" error in tests
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

    let prometheus_handle = get_prometheus_handle();

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
        prometheus_handle.clone(),
        Arc::new(Mutex::new(None)),
        Arc::new(Mutex::new({
            let mut r = StateResetOrchestrator::new(128);
            r.initialize();
            r
        })),
    )
}

// ── HTTP Handling Tests ──

#[tokio::test]
async fn test_get_request_returns_200() {
    let app = build_test_app(Arc::new(MockEngine));
    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_post_request_returns_200() {
    let app = build_test_app(Arc::new(MockEngine));
    let body = Body::from("test=post&data=value");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

// ── Health Probe Tests ──

#[tokio::test]
async fn test_health_endpoint_always_ok() {
    let app = build_test_app(Arc::new(MockEngine));
    let request = Request::builder()
        .uri("/health")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_ready_endpoint_not_ready_initially() {
    let app = build_test_app(Arc::new(MockEngine));
    let request = Request::builder()
        .uri("/ready")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

// ── Error Handling Tests ──

#[tokio::test]
async fn test_engine_failure_returns_502() {
    let app = build_test_app(Arc::new(FailingMockEngine));
    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn test_engine_timeout_maps_to_408() {
    let err = nusa_core::EngineError::Timeout;
    assert_eq!(err.to_http_status(), 408);
}

// ── Circuit Breaker Tests ──

#[tokio::test]
async fn test_circuit_breaker_opens_after_threshold() {
    let cb = Arc::new(CircuitBreaker::new(2, Duration::from_secs(10)));
    let resource_guard = ResourceGuard {
        max_request_bytes: 1024 * 1024,
        request_timeout_ms: 5000,
        max_concurrent: 5,
    };

    let prometheus_handle = get_prometheus_handle();

    let app = app(
        Arc::new(FailingMockEngine),
        Arc::new(PluginRegistry::new()),
        cb.clone(),
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
        prometheus_handle.clone(),
        Arc::new(Mutex::new(None)),
        Arc::new(Mutex::new({
            let mut r = StateResetOrchestrator::new(128);
            r.initialize();
            r
        })),
    );

    for _ in 0..2 {
        let request = Request::builder()
            .uri("/index.php")
            .method("GET")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    }

    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_circuit_breaker_recovers_after_timeout() {
    let cb = Arc::new(CircuitBreaker::new(1, Duration::from_millis(50)));
    cb.record_failure();
    assert!(!cb.allow_request(), "Circuit must be open");

    tokio::time::sleep(Duration::from_millis(60)).await;
    assert!(
        cb.allow_request(),
        "Circuit must be half-open after timeout"
    );

    cb.record_success();
    assert!(
        cb.allow_request(),
        "Circuit must be closed after success from half-open"
    );
}

// ── Backpressure Tests ──

#[tokio::test]
async fn test_backpressure_rejects_when_full() {
    let guard = BackpressureGuard::new(1);
    let permit = guard.try_acquire().await;
    assert!(permit.is_some(), "First permit must succeed");

    let permit2 = guard.try_acquire().await;
    assert!(
        permit2.is_none(),
        "Second permit must fail when at capacity"
    );
}

#[tokio::test]
async fn test_backpressure_releases_permit() {
    let guard = BackpressureGuard::new(1);
    {
        let permit = guard.try_acquire().await;
        assert!(permit.is_some());
    }

    let permit2 = guard.try_acquire().await;
    assert!(permit2.is_some(), "Permit must be available after drop");
}

// ── Resource Guard Tests ──

#[test]
fn test_validate_request_size_within_limit() {
    assert!(nusa_core::validate_request_size(Some(500), 1024));
}

#[test]
fn test_validate_request_size_exceeds_limit() {
    assert!(!nusa_core::validate_request_size(Some(2000), 1024));
}

#[test]
fn test_validate_request_size_no_content_length() {
    assert!(nusa_core::validate_request_size(None, 1024));
}

// ── HealthState Tests ──

#[test]
fn test_health_state_initially_not_ready() {
    let state = HealthState::new();
    assert!(!state.is_ready());
    assert_eq!(state.success_count(), 0);
    assert_eq!(state.error_count(), 0);
}

#[test]
fn test_health_state_mark_ready() {
    let state = HealthState::new();
    state.mark_ready();
    assert!(state.is_ready());
}

#[test]
fn test_health_state_records_success() {
    let state = HealthState::new();
    state.record_success();
    state.record_success();
    assert_eq!(state.success_count(), 2);
}

#[test]
fn test_health_state_records_error() {
    let state = HealthState::new();
    state.record_error();
    assert_eq!(state.error_count(), 1);
}

// ── Blueprint 6 Endpoint Tests ──

#[tokio::test]
async fn test_metrics_endpoint_returns_stub() {
    let app = build_test_app(Arc::new(MockEngine));
    let request = Request::builder()
        .uri("/metrics")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_ws_endpoint_requires_upgrade_headers() {
    let app = build_test_app(Arc::new(MockEngine));
    // WebSocket without upgrade headers should return 400 Bad Request
    let request = Request::builder()
        .uri("/ws")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_task_offload_endpoint_accepts() {
    let app = build_test_app(Arc::new(MockEngine));
    let body = Body::from(
        r#"{"HttpRequest":{"method":"GET","url":"http://example.com","headers":{},"body":null}}"#,
    );
    let request = Request::builder()
        .uri("/api/tasks")
        .method("POST")
        .header("content-type", "application/json")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
}
