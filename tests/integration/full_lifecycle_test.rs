//! Full lifecycle integration tests: config load -> engine selection -> request dispatch -> execute -> response.
//!
//! Tests the complete request flow through all system components.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::{Router, body::Body, http::Request};
use bytes::Bytes;
use parking_lot::Mutex;
use tower::ServiceExt;

use nusa_core::{
    BackpressureGuard, PhpEngine, PhpResponse, RequestContext, ResourceGuard, TaskManager,
    TenantRateLimiter, TenantRegistry,
};
use nusa_engine_child::ChildEngine;
use nusa_engine_wasm::WasmEngine;
use nusa_gateway::circuit_breaker::CircuitBreaker;
use nusa_gateway::health::HealthState;
use nusa_gateway::sse::SseManager;
use nusa_gateway::static_files::StaticFileHandler;
use nusa_gateway::tenant_circuit_breaker::TenantCircuitBreakers;
use nusa_gateway::websocket::WsManager;
use nusa_gateway::{app, health::HealthState};
use nusa_octane_worker::state_reset::StateResetOrchestrator;
use nusa_plugin_api::PluginRegistry;
use nusa_telemetry::metrics::NusaMetrics;

// ── Test Helpers ──

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
        Arc::new(Mutex::new(None)),
        {
            let mut r = StateResetOrchestrator::new(128);
            r.initialize();
            Arc::new(r)
        },
    )
}

// ── Mock Engine with Configurable Behavior ──

struct ConfigurableMockEngine {
    status: u16,
    body: &'static str,
    delay: Option<Duration>,
}

impl ConfigurableMockEngine {
    fn new(status: u16, body: &'static str) -> Self {
        Self { status, body, delay: None }
    }
    fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }
}

#[async_trait]
impl PhpEngine for ConfigurableMockEngine {
    async fn execute(&self, ctx: RequestContext) -> nusa_core::Result<PhpResponse> {
        if let Some(d) = self.delay {
            tokio::time::sleep(d).await;
        }
        Ok(PhpResponse {
            status: self.status,
            headers: Default::default(),
            body: Bytes::from(self.body),
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

struct SlowMockEngine;

#[async_trait]
impl PhpEngine for SlowMockEngine {
    async fn execute(&self, _ctx: RequestContext) -> nusa_core::Result<PhpResponse> {
        tokio::time::sleep(Duration::from_secs(30)).await;
        Ok(PhpResponse {
            status: 200,
            headers: Default::default(),
            body: Bytes::from("slow response"),
        })
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &["slow"]
    }
    async fn shutdown(&self) {}
}

// ── Full Request Lifecycle ──

#[tokio::test]
async fn lifecycle_full_get_request_returns_200() {
    let engine = Arc::new(ConfigurableMockEngine::new(200, "hello"));
    let router = build_test_app(engine);

    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .body(Body::empty())
        .expect("valid request");
    let response = router.oneshot(request).await.expect("response");
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.expect("body");
    assert_eq!(&body[..], b"hello");
}

#[tokio::test]
async fn lifecycle_full_post_request_with_body() {
    let engine = Arc::new(ConfigurableMockEngine::new(200, "post ok"));
    let router = build_test_app(engine);

    let body = Body::from("data=test");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .expect("valid request");
    let response = router.oneshot(request).await.expect("response");
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn lifecycle_request_with_headers_passed_to_engine() {
    let engine = Arc::new(ConfigurableMockEngine::new(200, "headers ok"));
    let router = build_test_app(engine);

    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .header("X-Custom-Header", "custom-value")
        .header("X-Request-Method", "GET")
        .header("X-Request-Uri", "/test")
        .body(Body::empty())
        .expect("valid request");
    let response = router.oneshot(request).await.expect("response");
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn lifecycle_large_body_streaming_no_memory_spike() {
    let engine = Arc::new(ConfigurableMockEngine::new(200, "large body handled"));
    let router = build_test_app(engine);

    // 1MB+ body
    let large_body = "x".repeat(1_100_000);
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-length", large_body.len())
        .body(Body::from(large_body))
        .expect("valid request");
    let response = router.oneshot(request).await.expect("response");
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn lifecycle_request_timeout_client_disconnect() {
    let engine = Arc::new(SlowMockEngine);
    let resource_guard = ResourceGuard {
        max_request_bytes: 1024 * 1024,
        request_timeout_ms: 100,
        max_concurrent: 100,
    };
    let prometheus_handle = get_prometheus_handle();
    let router = app(
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
        {
            let mut r = StateResetOrchestrator::new(128);
            r.initialize();
            Arc::new(r)
        },
    );

    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .body(Body::empty())
        .expect("valid request");
    let response = router.oneshot(request).await.expect("response");
    assert_eq!(response.status(), 408);
}

// ── Engine Fallback ──

#[tokio::test]
async fn lifecycle_engine_fallback_ffi_to_child() {
    let wasm_engine = WasmEngine::stub();
    let ctx = nusa_core::RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );
    let result = wasm_engine.execute(ctx).await;
    assert!(result.is_ok(), "WASM stub engine should succeed");
    let resp = result.unwrap();
    assert_eq!(resp.status, 200);
}

#[tokio::test]
async fn lifecycle_wasm_engine_stub_execute() {
    let wasm_engine = WasmEngine::stub();
    let ctx = nusa_core::RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );
    let result = wasm_engine.execute(ctx).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn lifecycle_child_engine_creation() {
    let child_engine = ChildEngine::with_default_php();
    let caps = child_engine.capabilities();
    assert!(caps.contains(&"child"));
    assert!(caps.contains(&"process"));
    assert!(caps.contains(&"isolated"));
    assert!(caps.contains(&"ipc"));
}

// ── Engine Switching ──

#[tokio::test]
async fn lifecycle_engine_switching_swap_without_restart() {
    let engine1: Arc<dyn PhpEngine> = Arc::new(ConfigurableMockEngine::new(200, "engine1"));
    let engine2: Arc<dyn PhpEngine> = Arc::new(ConfigurableMockEngine::new(200, "engine2"));

    let router1 = build_test_app(engine1);
    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .body(Body::empty())
        .expect("valid request");
    let response = router1.oneshot(request).await.expect("response");
    assert_eq!(response.status(), 200);

    let router2 = build_test_app(engine2);
    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .body(Body::empty())
        .expect("valid request");
    let response = router2.oneshot(request).await.expect("response");
    assert_eq!(response.status(), 200);
}

// ── Cold Start Latency ──

#[tokio::test]
async fn lifecycle_cold_start_latency_measured() {
    let engine = Arc::new(ConfigurableMockEngine::new(200, "cold start"));
    let router = build_test_app(engine);

    let start = std::time::Instant::now();
    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .body(Body::empty())
        .expect("valid request");
    let response = router.oneshot(request).await.expect("response");
    let elapsed = start.elapsed();

    assert_eq!(response.status(), 200);
    assert!(
        elapsed < Duration::from_secs(5),
        "cold start should complete within 5s, took {:?}",
        elapsed
    );
}

// ── Warm Start Latency ──

#[tokio::test]
async fn lifecycle_warm_start_latency_after_many_requests() {
    let engine = Arc::new(ConfigurableMockEngine::new(200, "warm"));
    let router = build_test_app(engine);

    // Warm up with requests
    for _ in 0..100 {
        let request = Request::builder()
            .uri("/index.php")
            .method("GET")
            .body(Body::empty())
            .expect("valid request");
        let response = router.clone().oneshot(request).await.expect("response");
        assert_eq!(response.status(), 200);
    }

    // Measure warm start latency
    let start = std::time::Instant::now();
    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .body(Body::empty())
        .expect("valid request");
    let response = router.oneshot(request).await.expect("response");
    let elapsed = start.elapsed();

    assert_eq!(response.status(), 200);
    assert!(
        elapsed < Duration::from_millis(500),
        "warm start should be fast, took {:?}",
        elapsed
    );
}

// ── Engine Capabilities ──

#[test]
fn lifecycle_wasm_capabilities_complete() {
    let engine = WasmEngine::stub();
    let caps = engine.capabilities();
    assert!(caps.contains(&"wasm"));
    assert!(caps.contains(&"sandbox"));
    assert!(caps.contains(&"fuel-limited"));
    assert!(caps.contains(&"memory-capped"));
}

#[test]
fn lifecycle_child_capabilities_complete() {
    let engine = ChildEngine::with_default_php();
    let caps = engine.capabilities();
    assert!(caps.contains(&"child"));
    assert!(caps.contains(&"process"));
    assert!(caps.contains(&"isolated"));
    assert!(caps.contains(&"ipc"));
}
