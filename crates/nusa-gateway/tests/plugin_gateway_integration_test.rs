//! Gateway plugin hook integration (sector S02 extension).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::body::to_bytes;
use axum::{Router, body::Body, http::Request};
use tower::ServiceExt;

use nusa_core::{
    BackpressureGuard, EngineError, PhpEngine, PhpResponse, RequestContext, ResourceGuard, Result,
    TaskManager, TenantRateLimiter, TenantRegistry,
};
use nusa_gateway::app;
use nusa_gateway::circuit_breaker::CircuitBreaker;
use nusa_gateway::health::HealthState;
use nusa_gateway::sse::SseManager;
use nusa_gateway::static_files::StaticFileHandler;
use nusa_gateway::tenant_circuit_breaker::TenantCircuitBreakers;
use nusa_gateway::websocket::WsManager;
use nusa_octane_worker::state_reset::StateResetOrchestrator;
use nusa_plugin_api::{Plugin, PluginRegistry};
use nusa_telemetry::metrics::NusaMetrics;

use std::sync::OnceLock;

static PROMETHEUS_HANDLE: OnceLock<Arc<metrics_exporter_prometheus::PrometheusHandle>> =
    OnceLock::new();

fn prometheus_handle() -> Arc<metrics_exporter_prometheus::PrometheusHandle> {
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

struct OkEngine;

#[async_trait]
impl PhpEngine for OkEngine {
    async fn execute(&self, _ctx: RequestContext) -> Result<PhpResponse> {
        Ok(PhpResponse::ok(200, b"ok".to_vec()))
    }

    fn capabilities(&self) -> &'static [&'static str] {
        &["ok"]
    }

    async fn shutdown(&self) {}
}

struct ShortCircuitPlugin;

#[async_trait]
impl Plugin for ShortCircuitPlugin {
    fn name(&self) -> &'static str {
        "short-circuit"
    }

    async fn pre_exec(&self, ctx: &mut RequestContext) -> Result<()> {
        ctx.set_short_circuit(PhpResponse::ok(418, b"teapot".to_vec()));
        Ok(())
    }
}

struct PanicEngine;

#[async_trait]
impl PhpEngine for PanicEngine {
    async fn execute(&self, _ctx: RequestContext) -> Result<PhpResponse> {
        panic!("engine must not run when Tier-S2 short-circuits");
    }

    fn capabilities(&self) -> &'static [&'static str] {
        &["panic"]
    }

    async fn shutdown(&self) {}
}

struct PreExecFailPlugin;

#[async_trait]
impl Plugin for PreExecFailPlugin {
    fn name(&self) -> &'static str {
        "pre-fail"
    }

    async fn pre_exec(&self, _ctx: &mut RequestContext) -> Result<()> {
        Err(EngineError::Plugin("blocked in pre_exec".into()))
    }
}

struct PostExecFailPlugin;

#[async_trait]
impl Plugin for PostExecFailPlugin {
    fn name(&self) -> &'static str {
        "post-fail"
    }

    async fn post_exec(&self, _ctx: &RequestContext) -> Result<()> {
        Err(EngineError::Plugin("blocked in post_exec".into()))
    }
}

fn build_app(engine: Arc<dyn PhpEngine>, plugins: Arc<PluginRegistry>) -> Router {
    let resource_guard = ResourceGuard {
        max_request_bytes: 1024 * 1024,
        request_timeout_ms: 5000,
        max_concurrent: 100,
    };
    let mut reset = StateResetOrchestrator::new(128);
    reset.initialize();
    app(
        engine,
        plugins,
        Arc::new(CircuitBreaker::new(10, Duration::from_secs(30))),
        Arc::new(HealthState::new()),
        Arc::new(BackpressureGuard::new(100)),
        resource_guard,
        Arc::new(TenantRegistry::new()),
        Arc::new(TaskManager::new()),
        Arc::new(TenantRateLimiter::new(1000, 50)),
        Arc::new(TenantCircuitBreakers::new(10, Duration::from_secs(30))),
        Arc::new(WsManager::new()),
        Arc::new(SseManager::new()),
        Arc::new(StaticFileHandler::new("/tmp".into())),
        Arc::new(NusaMetrics::init()),
        prometheus_handle(),
        Arc::new(tokio::sync::Mutex::new(None)),
        Arc::new(reset),
    )
}

async fn body_bytes(response: axum::response::Response) -> Vec<u8> {
    to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body")
        .to_vec()
}

#[tokio::test]
async fn gateway_pre_exec_short_circuit_skips_engine() {
    let plugins = Arc::new(PluginRegistry::new());
    plugins.register(Arc::new(ShortCircuitPlugin));
    let app = build_app(Arc::new(PanicEngine), plugins);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/dynamic")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), 418);
    let bytes = body_bytes(response).await;
    assert_eq!(String::from_utf8_lossy(&bytes), "teapot");
}

#[tokio::test]
async fn gateway_pre_exec_plugin_failure_returns_500() {
    let plugins = Arc::new(PluginRegistry::new());
    plugins.register(Arc::new(PreExecFailPlugin));
    let app = build_app(Arc::new(OkEngine), plugins);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), 500);
    let bytes = body_bytes(response).await;
    let body = String::from_utf8_lossy(&bytes);
    assert!(body.contains("Plugin Error"), "body was: {body}");
}

#[tokio::test]
async fn gateway_post_exec_plugin_failure_returns_500_after_engine_ok() {
    let plugins = Arc::new(PluginRegistry::new());
    plugins.register(Arc::new(PostExecFailPlugin));
    let app = build_app(Arc::new(OkEngine), plugins);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), 500);
    let bytes = body_bytes(response).await;
    let body = String::from_utf8_lossy(&bytes);
    assert!(body.contains("Plugin Error"), "body was: {body}");
}

struct CountingPrePlugin {
    count: Arc<std::sync::atomic::AtomicUsize>,
}

#[async_trait]
impl Plugin for CountingPrePlugin {
    fn name(&self) -> &'static str {
        "count-pre"
    }

    async fn pre_exec(&self, _ctx: &mut RequestContext) -> Result<()> {
        self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn gateway_runs_pre_exec_before_engine() {
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let plugins = Arc::new(PluginRegistry::new());
    plugins.register(Arc::new(CountingPrePlugin {
        count: count.clone(),
    }));
    let app = build_app(Arc::new(OkEngine), plugins);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), 200);
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
}
