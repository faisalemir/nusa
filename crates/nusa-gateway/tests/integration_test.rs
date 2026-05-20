//! Integration tests for nusa-gateway crate.
//!
//! Skills applied:
//! - `domain-web`: End-to-end HTTP request lifecycle
//! - `m06-error-handling`: Error propagation through full stack
//!
//! NOTE: Full integration tests are in gateway_integration_test.rs.
//! This file verifies middleware chain composition.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::{body::Body, http::Request, Router};
use bytes::Bytes;
use tower::ServiceExt;

use nusa_core::{BackpressureGuard, PhpEngine, PhpResponse, RequestContext, ResourceGuard, TaskManager, TenantRateLimiter, TenantRegistry};
use nusa_gateway::circuit_breaker::CircuitBreaker;
use nusa_gateway::health::HealthState;
use nusa_gateway::tenant_circuit_breaker::TenantCircuitBreakers;
use nusa_gateway::websocket::WsManager;
use nusa_gateway::sse::SseManager;
use nusa_gateway::static_files::StaticFileHandler;
use nusa_gateway::app;
use nusa_telemetry::metrics::NusaMetrics;
use nusa_plugin_api::PluginRegistry;

struct OkEngine;

#[async_trait]
impl PhpEngine for OkEngine {
    async fn execute(&self, _ctx: RequestContext) -> nusa_core::Result<PhpResponse> {
        Ok(PhpResponse {
            status: 200,
            headers: Default::default(),
            body: Bytes::from("OK"),
        })
    }
    fn capabilities(&self) -> &'static [&'static str] { &["mock"] }
    async fn shutdown(&self) {}
}

fn build_app() -> Router {
    let resource_guard = ResourceGuard {
        max_request_bytes: 1024 * 1024,
        request_timeout_ms: 5000,
        max_concurrent: 5,
    };
    app(
        Arc::new(OkEngine),
        Arc::new(PluginRegistry::new()),
        Arc::new(CircuitBreaker::new(5, Duration::from_secs(30))),
        Arc::new(HealthState::new()),
        Arc::new(BackpressureGuard::new(10)),
        resource_guard,
        Arc::new(TenantRegistry::new()),
        Arc::new(TaskManager::new()),
        Arc::new(TenantRateLimiter::new(1000, 50)),
        Arc::new(TenantCircuitBreakers::new(5, Duration::from_secs(30))),
        Arc::new(WsManager::new()),
        Arc::new(SseManager::new()),
        Arc::new(StaticFileHandler::new("/tmp".into())),
        Arc::new(NusaMetrics::init()),
    )
}

/// === Arrange ===
/// Full gateway app built with all middleware layers.
/// === Act ===
/// GET request to /health, /ready, /metrics, /ws endpoints.
/// === Assert ===
/// Each endpoint returns expected status code, middleware chain functional.
#[tokio::test]
async fn gateway_all_endpoints_respond() {
    // === Arrange ===
    let app = build_app();

    // === Act & Assert ===
    // Health endpoint
    let resp = app.clone().oneshot(
        Request::builder().uri("/health").method("GET").body(Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);

    // Ready endpoint (not yet marked ready)
    let resp = app.clone().oneshot(
        Request::builder().uri("/ready").method("GET").body(Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::SERVICE_UNAVAILABLE);

    // Metrics endpoint
    let resp = app.clone().oneshot(
        Request::builder().uri("/metrics").method("GET").body(Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);

    // WS endpoint (stub)
    let resp = app.clone().oneshot(
        Request::builder().uri("/ws").method("GET").body(Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::NOT_IMPLEMENTED);
}
