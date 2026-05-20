//! Nusa PHP Runtime Gateway — Axum HTTP server with middleware.
//!
//! Skills applied:
//! - `domain-web`: HTTP request handling, middleware chain, static files, WS, SSE
//! - `m13-domain-error`: Error → HTTP status mapping, circuit breaker, rate limiting
//! - `m07-concurrency`: Arc<dyn PhpEngine> shared across handlers
//! - `m05-type-driven`: TenantId enforced at gateway level
//! - `m09-domain`: Tenant isolation via registry checks
//! - `m10-performance`: Prometheus metrics, compression, static file cache

#![deny(unsafe_code)]
#![warn(clippy::all)]
#![allow(missing_docs)]

pub mod acme;
pub mod bluegreen;
pub mod broadcast;
pub mod circuit_breaker;
pub mod health;
pub mod middleware;
pub mod quic;
pub mod sse;
pub mod static_files;
pub mod tenant_circuit_breaker;
pub mod tls;
pub mod websocket;

use std::sync::Arc;
use std::time::Duration;

use crate::circuit_breaker::CircuitBreaker;
use crate::health::HealthState;
use crate::sse::SseManager;
use crate::static_files::StaticFileHandler;
use crate::tenant_circuit_breaker::TenantCircuitBreakers;
use crate::websocket::WsManager;
use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    response::Response,
};
use nusa_core::{
    BackpressureGuard, PhpEngine, PhpResponse, RequestContext, ResourceGuard, TaskManager,
    TenantRateLimiter, TenantRegistry, with_timeout,
};
use nusa_plugin_api::PluginRegistry;
use nusa_telemetry::metrics::NusaMetrics;

/// Shared application state (all fields Arc-wrapped for Clone + thread-safety).
#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<dyn PhpEngine>,
    pub plugins: Arc<PluginRegistry>,
    pub circuit_breaker: Arc<CircuitBreaker>,
    pub health_state: Arc<HealthState>,
    pub backpressure: Arc<BackpressureGuard>,
    pub resource_guard: Arc<ResourceGuard>,
    pub tenants: Arc<TenantRegistry>,
    pub tasks: Arc<TaskManager>,
    pub rate_limiter: Arc<TenantRateLimiter>,
    pub tenant_cb: Arc<TenantCircuitBreakers>,
    pub ws_manager: Arc<WsManager>,
    pub sse_manager: Arc<SseManager>,
    pub static_handler: Arc<StaticFileHandler>,
    pub metrics: Arc<NusaMetrics>,
}

/// Main application router with all Blueprint 6 endpoints.
#[allow(clippy::too_many_arguments)]
pub fn app(
    engine: Arc<dyn PhpEngine>,
    plugins: Arc<PluginRegistry>,
    circuit_breaker: Arc<CircuitBreaker>,
    health_state: Arc<HealthState>,
    backpressure: Arc<BackpressureGuard>,
    resource_guard: ResourceGuard,
    tenants: Arc<TenantRegistry>,
    tasks: Arc<TaskManager>,
    rate_limiter: Arc<TenantRateLimiter>,
    tenant_cb: Arc<TenantCircuitBreakers>,
    ws_manager: Arc<WsManager>,
    sse_manager: Arc<SseManager>,
    static_handler: Arc<StaticFileHandler>,
    metrics: Arc<NusaMetrics>,
) -> Router {
    let state = AppState {
        engine,
        plugins,
        circuit_breaker,
        health_state,
        backpressure,
        resource_guard: Arc::new(resource_guard),
        tenants,
        tasks,
        rate_limiter,
        tenant_cb,
        ws_manager,
        sse_manager,
        static_handler,
        metrics,
    };

    Router::new()
        // Health & Readiness
        .route("/health", axum::routing::get(health::health_handler))
        .route(
            "/ready",
            axum::routing::get({
                let hs = state.health_state.clone();
                move || async move {
                    if hs.is_ready() {
                        (StatusCode::OK, "READY")
                    } else {
                        (StatusCode::SERVICE_UNAVAILABLE, "Not Ready")
                    }
                }
            }),
        )
        // Prometheus metrics (A1)
        .route("/metrics", axum::routing::get(metrics_handler))
        // WebSocket (Blueprint 6 F1) — stub until axum ws feature
        .route("/ws", axum::routing::get(ws_upgrade_stub_handler))
        // SSE (Blueprint 6 F2)
        .route("/sse", axum::routing::get(sse_handler))
        // Task offload (M4)
        .route("/api/tasks", axum::routing::post(offload_task_handler))
        .route(
            "/api/tasks/{task_id}/status",
            axum::routing::get(task_status_handler),
        )
        // Static files (Blueprint 6 E1)
        .route(
            "/static/{*path}",
            axum::routing::get(|_path: axum::extract::Path<String>| async move {
                (
                    StatusCode::NOT_IMPLEMENTED,
                    "Static file serving not yet implemented",
                )
            }),
        )
        // Catch-all route for PHP scripts
        .route("/{*path}", axum::routing::get(handler).post(handler))
        // Middleware Chain: trace, CORS, compression, size limit
        .layer(
            tower::ServiceBuilder::new()
                .layer(tower_http::trace::TraceLayer::new_for_http())
                .layer(tower_http::cors::CorsLayer::permissive())
                .layer(tower_http::compression::CompressionLayer::new())
                .layer(axum::middleware::from_fn(middleware::request_size_limit)),
        )
        .with_state(state)
}

/// Prometheus metrics endpoint stub (A1).
///
/// Status: Stub. Returns placeholder until real Prometheus metrics integration is wired.
async fn metrics_handler() -> String {
    "# Nusa runtime metrics (see :9090/metrics for full prometheus export)\n".into()
}

/// WebSocket upgrade stub (Blueprint 6 F1).
///
/// Status: Stub (requires axum `ws` feature for real WebSocket support).
async fn ws_upgrade_stub_handler() -> (StatusCode, &'static str) {
    (
        StatusCode::NOT_IMPLEMENTED,
        "WebSocket support requires axum 'ws' feature.",
    )
}

/// SSE handler route (Blueprint 6 F2).
async fn sse_handler(
    State(state): State<AppState>,
) -> axum::response::Sse<
    impl futures::stream::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>
    + use<>,
> {
    axum::response::Sse::new(state.sse_manager.stream()).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// Task offload handler (M4).
async fn offload_task_handler(
    State(state): State<AppState>,
    Json(task): Json<nusa_core::OffloadTask>,
) -> (StatusCode, Json<serde_json::Value>) {
    let (task_id, _rx) = state.tasks.submit(task);
    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "task_id": task_id })),
    )
}

/// Task status handler (M4).
async fn task_status_handler(
    State(state): State<AppState>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let status = state.tasks.status(&task_id);
    match serde_json::to_value(status) {
        Ok(val) => (StatusCode::OK, Json(val)),
        Err(e) => {
            tracing::error!(err = %e, "failed to serialize task status");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "serialization failed" })),
            )
        }
    }
}

/// Main Request Handler (m13-domain-error: EngineError -> HTTP status).
async fn handler(State(state): State<AppState>, req: Request<Body>) -> Response<Body> {
    state.metrics.requests_total.increment(1);
    let start = std::time::Instant::now();

    let method = req.method().clone();
    let uri = req.uri().clone();
    let trace_id = middleware::extract_trace_id(req.headers());

    let span = tracing::info_span!("request", method = %method, uri = %uri, trace_id = %trace_id);
    let _enter = span.enter();

    // Check circuit breaker
    if !state.circuit_breaker.allow_request() {
        tracing::warn!("circuit breaker open, rejecting request");
        state.metrics.requests_failed_total.increment(1);
        state.health_state.record_error();
        return Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .body(Body::from("Service Unavailable: Circuit Open"))
            .expect("builder with valid status always succeeds");
    }

    // Check backpressure
    let permit = match state.backpressure.try_acquire().await {
        Some(p) => p,
        None => {
            tracing::warn!("backpressure limit reached, rejecting request");
            state.metrics.requests_failed_total.increment(1);
            state.health_state.record_error();
            return Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .body(Body::from("Service Unavailable: Too Many Requests"))
                .expect("builder with valid status always succeeds");
        }
    };

    // Build request context
    let ctx = match build_request_context(req, &state.resource_guard).await {
        Ok(ctx) => ctx,
        Err(status) => {
            state.metrics.requests_failed_total.increment(1);
            return Response::builder()
                .status(status)
                .body(Body::from("Bad Request"))
                .expect("builder with valid status always succeeds");
        }
    };

    // D2: Check tenant rate limit
    let tenant_id_for_cb = ctx.tenant_id().cloned();
    if let Some(tenant_id) = &tenant_id_for_cb {
        if !state.rate_limiter.is_allowed(tenant_id) {
            state.metrics.requests_failed_total.increment(1);
            return Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .header("Retry-After", "60")
                .body(Body::from("Too Many Requests: Rate Limit Exceeded"))
                .expect("builder with valid status always succeeds");
        }

        // D3: Check per-tenant circuit breaker
        if !state.tenant_cb.is_allowed(tenant_id) {
            state.metrics.requests_failed_total.increment(1);
            return Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .body(Body::from("Service Unavailable: Tenant Circuit Open"))
                .expect("builder with valid status always succeeds");
        }

        // Check tenant is enabled
        if !state.tenants.is_enabled(tenant_id) {
            state.metrics.requests_failed_total.increment(1);
            return Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Body::from("Tenant not enabled"))
                .expect("builder with valid status always succeeds");
        }
    }

    // Execute PHP Engine with timeout
    match with_timeout(
        state.resource_guard.request_timeout_ms,
        state.engine.execute(ctx),
    )
    .await
    {
        Ok(res) => {
            state.circuit_breaker.record_success();
            if let Some(tid) = &tenant_id_for_cb {
                state.tenant_cb.record_success(tid);
            }
            state.health_state.record_success();
            drop(permit);
            state
                .metrics
                .request_duration_ms
                .record(start.elapsed().as_secs_f64() * 1000.0);
            build_http_response(res)
        }
        Err(e) => {
            state.circuit_breaker.record_failure();
            if let Some(tid) = &tenant_id_for_cb {
                state.tenant_cb.record_failure(tid);
            }
            state.metrics.requests_failed_total.increment(1);
            state.health_state.record_error();
            drop(permit);
            tracing::error!(err = %e, "engine execution failed");
            let status = e.to_http_status();
            Response::builder()
                .status(status)
                .body(Body::from(format!("Upstream Error: {e}")))
                .expect("builder with valid status always succeeds")
        }
    }
}

/// Build RequestContext from HTTP request.
async fn build_request_context(
    req: Request<Body>,
    guard: &ResourceGuard,
) -> Result<RequestContext, StatusCode> {
    let content_length = req
        .headers()
        .get(http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    if !nusa_core::validate_request_size(content_length, guard.max_request_bytes) {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }

    let headers = req.headers().clone();
    let tenant = middleware::extract_tenant(&headers);

    let body_bytes = axum::body::to_bytes(req.into_body(), guard.max_request_bytes)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let mut ctx = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_millis(guard.request_timeout_ms),
    )
    .with_body(body_bytes)
    .with_headers(headers)
    .with_env(Arc::new(std::env::vars().collect()));

    if let Some(t) = tenant {
        ctx = ctx.with_tenant(t);
    }

    Ok(ctx)
}

/// Convert PhpResponse to HTTP Response.
fn build_http_response(php_resp: PhpResponse) -> Response<Body> {
    Response::builder()
        .status(php_resp.status)
        .body(Body::from(php_resp.body.to_vec()))
        .expect("builder with valid status always succeeds")
}
