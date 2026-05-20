//! Detailed integration tests for phprt-gateway
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
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use bytes::Bytes;
use tower::ServiceExt;

use phprt_core::{
    PhpEngine, PhpResponse, RequestContext,
    BackpressureGuard, ResourceGuard, TenantRegistry, TaskManager,
};
use phprt_gateway::{app, health::HealthState};
use phprt_gateway::circuit_breaker::CircuitBreaker;
use phprt_plugin_api::PluginRegistry;

// ── Mock Engines ──

struct MockEngine;

#[async_trait]
impl PhpEngine for MockEngine {
    async fn execute(&self, _ctx: RequestContext) -> phprt_core::Result<PhpResponse> {
        Ok(PhpResponse {
            status: 200,
            headers: Default::default(),
            body: Bytes::from("Mock PHP Response"),
        })
    }
    fn capabilities(&self) -> &'static [&'static str] { &["mock"] }
    async fn shutdown(&self) {}
}

struct FailingMockEngine;

#[async_trait]
impl PhpEngine for FailingMockEngine {
    async fn execute(&self, _ctx: RequestContext) -> phprt_core::Result<PhpResponse> {
        Err(phprt_core::EngineError::PhpFatal("Test failure".into()))
    }
    fn capabilities(&self) -> &'static [&'static str] { &["mock"] }
    async fn shutdown(&self) {}
}

// ── Test Helpers ──

fn build_test_app(engine: Arc<dyn PhpEngine>) -> Router {
    app(
        engine,
        Arc::new(PluginRegistry::new()),
        Arc::new(CircuitBreaker::new(3, Duration::from_secs(1))),
        Arc::new(HealthState::new()),
        Arc::new(BackpressureGuard::new(100)),
        ResourceGuard {
            max_request_bytes: 1024,
            request_timeout_ms: 5000,
            max_concurrent: 5,
        },
        Arc::new(TenantRegistry::new()),
        Arc::new(TaskManager::new()),
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

#[tokio::test]
async fn test_ready_endpoint_becomes_ready_after_mark() {
    let health = Arc::new(HealthState::new());
    health.mark_ready();

    let app = app(
        Arc::new(MockEngine),
        Arc::new(PluginRegistry::new()),
        Arc::new(CircuitBreaker::new(3, Duration::from_secs(1))),
        health.clone(),
        Arc::new(BackpressureGuard::new(100)),
        ResourceGuard::default(),
        Arc::new(TenantRegistry::new()),
        Arc::new(TaskManager::new()),
    );

    let request = Request::builder()
        .uri("/ready")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
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
    let err = phprt_core::EngineError::Timeout;
    assert_eq!(err.to_http_status(), 408);
}

// ── Circuit Breaker Tests ──

#[tokio::test]
async fn test_circuit_breaker_opens_after_threshold() {
    let cb = Arc::new(CircuitBreaker::new(2, Duration::from_secs(10)));
    let app = app(
        Arc::new(FailingMockEngine),
        Arc::new(PluginRegistry::new()),
        cb.clone(),
        Arc::new(HealthState::new()),
        Arc::new(BackpressureGuard::new(100)),
        ResourceGuard::default(),
        Arc::new(TenantRegistry::new()),
        Arc::new(TaskManager::new()),
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
    assert!(cb.allow_request(), "Circuit must be half-open after timeout");

    cb.record_success();
    assert!(cb.allow_request(), "Circuit must be closed after success from half-open");
}

// ── Backpressure Tests ──

#[tokio::test]
async fn test_backpressure_rejects_when_full() {
    let guard = BackpressureGuard::new(1);
    let permit = guard.try_acquire().await;
    assert!(permit.is_some(), "First permit must succeed");

    let permit2 = guard.try_acquire().await;
    assert!(permit2.is_none(), "Second permit must fail when at capacity");
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
    assert!(phprt_core::validate_request_size(Some(500), 1024));
}

#[test]
fn test_validate_request_size_exceeds_limit() {
    assert!(!phprt_core::validate_request_size(Some(2000), 1024));
}

#[test]
fn test_validate_request_size_no_content_length() {
    assert!(phprt_core::validate_request_size(None, 1024));
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
