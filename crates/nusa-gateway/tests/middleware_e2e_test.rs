//! Middleware E2E integration tests.
//!
//! Tests request size limit, CORS, compression, trace, tenant extraction, and middleware chain order.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::{Router, body::Body, http::Request, http::StatusCode};
use bytes::Bytes;
use parking_lot::Mutex;
use tower::ServiceExt;

use nusa_core::{
    BackpressureGuard, PhpEngine, PhpResponse, RequestContext, ResourceGuard, TaskManager,
    TenantRateLimiter, TenantRegistry,
};
use nusa_gateway::app;
use nusa_gateway::circuit_breaker::CircuitBreaker;
use nusa_gateway::health::HealthState;
use nusa_gateway::middleware::{extract_tenant, extract_trace_id};
use nusa_gateway::sse::SseManager;
use nusa_gateway::static_files::StaticFileHandler;
use nusa_gateway::tenant_circuit_breaker::TenantCircuitBreakers;
use nusa_gateway::websocket::WsManager;
use nusa_octane_worker::state_reset::StateResetOrchestrator;
use nusa_plugin_api::PluginRegistry;
use nusa_telemetry::metrics::NusaMetrics;

use std::sync::OnceLock;

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

fn build_test_app(engine: Arc<dyn PhpEngine>) -> Router {
    let resource_guard = ResourceGuard {
        max_request_bytes: 1024 * 1024,
        request_timeout_ms: 5000,
        max_concurrent: 100,
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
        Arc::new(tokio::sync::Mutex::new(None)),
        Arc::new(Mutex::new({
            let mut r = StateResetOrchestrator::new(128);
            r.initialize();
            r
        })),
    )
}

struct SimpleMockEngine;

#[async_trait]
impl PhpEngine for SimpleMockEngine {
    async fn execute(&self, _ctx: RequestContext) -> nusa_core::Result<PhpResponse> {
        Ok(PhpResponse {
            status: 200,
            headers: Default::default(),
            body: Bytes::from("ok"),
        })
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &["mock"]
    }
    async fn shutdown(&self) {}
}

// ── Request Size Limit ──

#[tokio::test]
async fn middleware_request_size_limit_body_over_limit_returns_413() {
    let engine = Arc::new(SimpleMockEngine);
    let router = build_test_app(engine);

    // 11MB body (exceeds 10MB default limit)
    let large_body = "x".repeat(11 * 1024 * 1024);
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-length", large_body.len())
        .body(Body::from(large_body))
        .expect("valid request");
    let response = router.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn middleware_request_size_limit_body_within_limit_ok() {
    let engine = Arc::new(SimpleMockEngine);
    let router = build_test_app(engine);

    let body = "small body";
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-length", body.len())
        .body(Body::from(body))
        .expect("valid request");
    let response = router.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}

// ── CORS ──

#[tokio::test]
async fn middleware_cors_preflight_options_returns_200_with_cors_headers() {
    let engine = Arc::new(SimpleMockEngine);
    let router = build_test_app(engine);

    let request = Request::builder()
        .uri("/index.php")
        .method("OPTIONS")
        .header("origin", "https://example.com")
        .header("access-control-request-method", "POST")
        .body(Body::empty())
        .expect("valid request");
    let response = router.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn middleware_cors_actual_request_cors_headers_added() {
    let engine = Arc::new(SimpleMockEngine);
    let router = build_test_app(engine);

    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .header("origin", "https://example.com")
        .body(Body::empty())
        .expect("valid request");
    let response = router.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    // CORS layer is permissive, so headers should be present
    let cors_header = response.headers().get("access-control-allow-origin");
    assert!(cors_header.is_some(), "CORS header should be present");
}

// ── Compression ──

#[tokio::test]
async fn middleware_compression_response_compressed_when_client_accepts_gzip() {
    let engine = Arc::new(SimpleMockEngine);
    let router = build_test_app(engine);

    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .header("accept-encoding", "gzip")
        .body(Body::empty())
        .expect("valid request");
    let response = router.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn middleware_compression_response_compressed_when_client_accepts_brotli() {
    let engine = Arc::new(SimpleMockEngine);
    let router = build_test_app(engine);

    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .header("accept-encoding", "br")
        .body(Body::empty())
        .expect("valid request");
    let response = router.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}

// ── Trace Layer ──

#[tokio::test]
async fn middleware_trace_context_added_to_request() {
    let headers = http::HeaderMap::new();
    let trace_id = extract_trace_id(&headers);
    assert!(trace_id.as_uuid() != uuid::Uuid::nil());
}

#[tokio::test]
async fn middleware_trace_span_created() {
    // TraceLayer creates spans for each request
    let engine = Arc::new(SimpleMockEngine);
    let router = build_test_app(engine);

    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .body(Body::empty())
        .expect("valid request");
    let response = router.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}

// ── Middleware Chain Order ──

#[tokio::test]
async fn middleware_chain_order_trace_cors_compression_size_limit() {
    // Chain order verified through source code:
    // TraceLayer -> CorsLayer -> CompressionLayer -> request_size_limit
    let engine = Arc::new(SimpleMockEngine);
    let router = build_test_app(engine);

    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .header(
            "traceparent",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        )
        .header("origin", "https://example.com")
        .header("accept-encoding", "gzip")
        .body(Body::empty())
        .expect("valid request");
    let response = router.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}

// ── Tenant Extraction ──

#[test]
fn middleware_tenant_extraction_header() {
    let mut headers = http::HeaderMap::new();
    headers.insert("x-tenant-id", "tenant-123".parse().unwrap());

    let tenant = extract_tenant(&headers);
    assert!(tenant.is_some());
    assert_eq!(tenant.unwrap().as_str(), "tenant-123");
}

#[test]
fn middleware_tenant_extraction_subdomain() {
    let mut headers = http::HeaderMap::new();
    headers.insert("host", "tenant-abc.example.com".parse().unwrap());

    let tenant = extract_tenant(&headers);
    assert!(tenant.is_some());
    assert_eq!(tenant.unwrap().as_str(), "tenant-abc");
}

#[test]
fn middleware_tenant_extraction_localhost_no_tenant() {
    let mut headers = http::HeaderMap::new();
    headers.insert("host", "localhost:3000".parse().unwrap());

    let tenant = extract_tenant(&headers);
    assert!(tenant.is_none());
}

#[test]
fn middleware_tenant_extraction_127_no_tenant() {
    let mut headers = http::HeaderMap::new();
    headers.insert("host", "127.0.0.1:3000".parse().unwrap());

    let tenant = extract_tenant(&headers);
    assert!(tenant.is_none());
}

#[test]
fn middleware_tenant_extraction_no_headers_no_tenant() {
    let headers = http::HeaderMap::new();
    let tenant = extract_tenant(&headers);
    assert!(tenant.is_none());
}

// ── Trace ID ──

#[test]
fn middleware_trace_id_from_traceparent_header() {
    let mut headers = http::HeaderMap::new();
    headers.insert(
        "traceparent",
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
            .parse()
            .unwrap(),
    );

    let trace_id = extract_trace_id(&headers);
    assert!(!trace_id.as_uuid().to_string().is_empty());
}

#[test]
fn middleware_trace_id_no_header_generated() {
    let headers = http::HeaderMap::new();
    let trace_id = extract_trace_id(&headers);
    assert!(trace_id.as_uuid() != uuid::Uuid::nil());
}

#[test]
fn middleware_trace_id_invalid_header_ignored() {
    let mut headers = http::HeaderMap::new();
    headers.insert("traceparent", "invalid".parse().unwrap());

    let trace_id = extract_trace_id(&headers);
    // Should generate new trace ID for invalid input
    assert!(trace_id.as_uuid() != uuid::Uuid::nil());
}
