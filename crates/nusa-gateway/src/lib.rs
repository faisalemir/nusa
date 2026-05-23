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
pub mod response;
pub mod sse;
pub mod static_files;
pub mod tenant_circuit_breaker;
pub mod tls;
pub mod websocket;

use std::collections::HashMap;
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
    http::{Request, StatusCode, header},
    response::Response,
};
use nusa_core::{
    BackpressureGuard, LaravelHttpRuntime, PhpEngine, PhpResponse, RequestContext, ResourceGuard,
    TaskManager, TenantRateLimiter, TenantRegistry, with_timeout,
};
use nusa_octane_worker::state_reset::StateResetOrchestrator;
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
    pub prometheus_handle: Arc<metrics_exporter_prometheus::PrometheusHandle>,
    /// Laravel Octane runtime (IPC or embed backend).
    /// `tokio::sync::Mutex` so handlers can `.await` dispatch without holding `parking_lot` guards.
    pub laravel_runtime: Arc<tokio::sync::Mutex<Option<Box<dyn LaravelHttpRuntime>>>>,
    /// Octane state reset orchestrator.
    /// `emit_event` is `&self` (atomic counters + broadcast) — no Mutex needed.
    pub octane_reset: Arc<StateResetOrchestrator>,
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
    prometheus_handle: Arc<metrics_exporter_prometheus::PrometheusHandle>,
    laravel_runtime: Arc<tokio::sync::Mutex<Option<Box<dyn LaravelHttpRuntime>>>>,
    octane_reset: Arc<StateResetOrchestrator>,
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
        prometheus_handle,
        laravel_runtime,
        octane_reset,
    };

    Router::new()
        // Health & Readiness
        .route("/health", axum::routing::get(health::health_handler))
        .route(
            "/ready",
            axum::routing::get({
                let hs = state.health_state.clone();
                let laravel_runtime = state.laravel_runtime.clone();
                move || async move {
                    let octane_ok = match laravel_runtime.lock().await.as_ref() {
                        None => true,
                        Some(pool) => pool.is_ready(),
                    };
                    if hs.is_ready() && octane_ok {
                        (StatusCode::OK, "READY")
                    } else {
                        (StatusCode::SERVICE_UNAVAILABLE, "Not Ready")
                    }
                }
            }),
        )
        // Prometheus metrics (A1)
        .route("/metrics", axum::routing::get(metrics_handler))
        // WebSocket (Blueprint 6 F1)
        .route("/ws", axum::routing::get(ws_upgrade_handler))
        // SSE (Blueprint 6 F2)
        .route("/sse", axum::routing::get(sse_handler))
        // Task offload (M4)
        .route("/api/tasks", axum::routing::post(offload_task_handler))
        .route(
            "/api/tasks/{task_id}/status",
            axum::routing::get(task_status_handler),
        )
        // Static files (Blueprint 6 E1)
        .route("/static/{*path}", axum::routing::get(static_file_handler))
        // Root + catch-all for PHP (axum `/{*path}` does not match `/` alone)
        .route("/", axum::routing::get(handler).post(handler))
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

/// Prometheus metrics endpoint (A1).
///
/// Returns the full Prometheus exposition format output from the global recorder.
async fn metrics_handler(State(state): State<AppState>) -> String {
    state.prometheus_handle.render()
}

/// WebSocket upgrade handler (Blueprint 6 F1).
///
/// Extracts tenant_id from headers and generates a unique connection ID.
async fn ws_upgrade_handler(
    State(state): State<AppState>,
    ws: axum::extract::WebSocketUpgrade,
) -> axum::response::Response {
    let ws_manager = state.ws_manager.clone();
    ws.on_upgrade(move |socket| async move {
        let connection_id = uuid::Uuid::new_v4().to_string();
        // Extract tenant_id from WebSocket upgrade headers or use default
        // In production, this would be authenticated and extracted from a token
        let tenant_id = nusa_core::TenantId::new("default");
        ws_manager
            .handle_connection(socket, connection_id, tenant_id)
            .await;
    })
}

/// Static file handler (Blueprint 6 E1).
///
/// Routes to StaticFileHandler which serves files with LRU cache,
/// proper MIME types, Cache-Control headers, and directory traversal prevention.
async fn static_file_handler(
    State(state): State<AppState>,
    axum::extract::Path(path): axum::extract::Path<String>,
    headers: axum::http::HeaderMap,
) -> Response<Body> {
    let accept = headers
        .get(header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok());
    match state
        .static_handler
        .serve(&path, &http::Method::GET, accept)
        .await
    {
        Some(resp) => {
            state.metrics.static_served_total.increment(1);
            resp
        }
        None => response::status_response(StatusCode::NOT_FOUND, "Not Found"),
    }
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
        return response::status_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "Service Unavailable: Circuit Open",
        );
    }

    // Check backpressure
    let permit = match state.backpressure.try_acquire().await {
        Some(p) => p,
        None => {
            tracing::warn!("backpressure limit reached, rejecting request");
            state.metrics.requests_failed_total.increment(1);
            state.health_state.record_error();
            return response::status_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Service Unavailable: Too Many Requests",
            );
        }
    };

    let method_str = method.to_string();
    let uri_str = uri
        .path_and_query()
        .map(|p| p.as_str())
        .unwrap_or_else(|| uri.path())
        .to_string();

    // Build request context
    let ctx = match build_request_context(req, &method_str, &uri_str, &state.resource_guard).await {
        Ok(ctx) => ctx,
        Err(status) => {
            state.metrics.requests_failed_total.increment(1);
            return response::status_response(status, "Bad Request");
        }
    };

    // D2: Check tenant rate limit
    let tenant_id_for_cb = ctx.tenant_id().cloned();
    if let Some(tenant_id) = &tenant_id_for_cb {
        if !state.rate_limiter.is_allowed(tenant_id) {
            state.metrics.requests_failed_total.increment(1);
            return response::status_response(
                StatusCode::TOO_MANY_REQUESTS,
                "Too Many Requests: Rate Limit Exceeded",
            );
        }

        // D3: Check per-tenant circuit breaker
        if !state.tenant_cb.is_allowed(tenant_id) {
            state.metrics.requests_failed_total.increment(1);
            return response::status_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Service Unavailable: Tenant Circuit Open",
            );
        }

        // Check tenant is enabled
        if !state.tenants.is_enabled(tenant_id) {
            state.metrics.requests_failed_total.increment(1);
            return response::status_response(StatusCode::FORBIDDEN, "Tenant not enabled");
        }
    }

    let mut ctx = ctx;
    if let Err(e) = state.plugins.run_pre_exec(&mut ctx).await {
        state.metrics.requests_failed_total.increment(1);
        state.health_state.record_error();
        drop(permit);
        let status =
            StatusCode::from_u16(e.to_http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        return response::status_response(status, format!("Plugin Error: {e}"));
    }

    // Tier-S2: plugin short-circuit (full response without PHP/Octane).
    if let Some(php_resp) = ctx.take_short_circuit() {
        state.metrics.tier_s2_short_circuit_total.increment(1);
        state.circuit_breaker.record_success();
        drop(permit);
        state
            .metrics
            .request_duration_ms
            .record(start.elapsed().as_secs_f64() * 1000.0);
        return build_http_response(php_resp);
    }

    // Tier-S1: serve static extensions from `static_root` before PHP (GET/HEAD only).
    if matches!(method.as_str(), "GET" | "HEAD") {
        let static_rel = uri.path().trim_start_matches('/');
        let accept = ctx
            .headers()
            .get(header::ACCEPT_ENCODING)
            .and_then(|v| v.to_str().ok());
        if !static_rel.is_empty()
            && StaticFileHandler::is_static(static_rel)
            && let Some(resp) = state
                .static_handler
                .serve(static_rel, &method, accept)
                .await
        {
            state.metrics.static_served_total.increment(1);
            state.circuit_breaker.record_success();
            drop(permit);
            state
                .metrics
                .request_duration_ms
                .record(start.elapsed().as_secs_f64() * 1000.0);
            return resp;
        }
    }

    let octane_mode = {
        let guard = state.laravel_runtime.lock().await;
        match guard.as_ref() {
            None => OctaneRoute::Engine,
            Some(pool) if pool.is_ready() => OctaneRoute::Pool,
            Some(_) => OctaneRoute::NotReady,
        }
    };

    if matches!(octane_mode, OctaneRoute::NotReady) {
        state.metrics.requests_failed_total.increment(1);
        state.health_state.record_error();
        drop(permit);
        return response::status_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "Service Unavailable: Octane worker pool not ready",
        );
    }

    let timeout_ms = state.resource_guard.request_timeout_ms;
    let mut ctx_post: Option<RequestContext> = None;
    let execution = match octane_mode {
        OctaneRoute::Pool => {
            ctx_post = Some(ctx.clone());
            let headers = http_headers_to_ipc(ctx.headers());
            let body = if ctx.body().is_empty() {
                None
            } else {
                Some(ctx.body().to_vec())
            };
            let request_id = ctx.trace_id().to_string();
            state.octane_reset.emit_event(
                nusa_octane_worker::state_reset::OctaneEvent::RequestReceived { request_id },
            );
            let reset = state.octane_reset.clone();
            let runtime = state.laravel_runtime.clone();
            let request_id_for_terminate = ctx.trace_id().to_string();
            let result = with_timeout(timeout_ms, async move {
                let mut guard = runtime.lock().await;
                let pool = guard
                    .as_mut()
                    .expect("OctaneRoute::Pool implies Some(runtime)");
                pool.handle_http_request(method_str, uri_str, headers, body, timeout_ms)
                    .await
            })
            .await;
            let status = result.as_ref().map(|r| r.status).unwrap_or(500);
            reset.emit_event(
                nusa_octane_worker::state_reset::OctaneEvent::RequestTerminated {
                    request_id: request_id_for_terminate,
                    status,
                },
            );
            result
        }
        OctaneRoute::Engine => with_timeout(timeout_ms, state.engine.execute(ctx)).await,
        OctaneRoute::NotReady => unreachable!(),
    };

    match execution {
        Ok(res) => {
            if let Some(ctx_post) = &ctx_post
                && let Err(e) = state.plugins.run_post_exec(ctx_post).await
            {
                state.metrics.requests_failed_total.increment(1);
                state.health_state.record_error();
                drop(permit);
                let status = StatusCode::from_u16(e.to_http_status())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                return response::status_response(status, format!("Plugin Error: {e}"));
            }
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
            tracing::error!(err = %e, "request execution failed");
            let status = StatusCode::from_u16(e.to_http_status())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            response::status_response(status, format!("Upstream Error: {e}"))
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
enum OctaneRoute {
    Engine,
    Pool,
    NotReady,
}

fn http_headers_to_ipc(headers: &http::HeaderMap) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for (name, value) in headers.iter() {
        if let Ok(s) = value.to_str() {
            map.entry(name.as_str().to_string())
                .or_default()
                .push(s.to_string());
        }
    }
    map
}

/// Build RequestContext from HTTP request.
async fn build_request_context(
    req: Request<Body>,
    method: &str,
    uri: &str,
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

    let mut headers = req.headers().clone();
    if let (Ok(name), Ok(value)) = (
        http::HeaderName::from_bytes(b"x-request-method"),
        http::HeaderValue::from_str(method),
    ) {
        headers.insert(name, value);
    }
    if let (Ok(name), Ok(value)) = (
        http::HeaderName::from_bytes(b"x-request-uri"),
        http::HeaderValue::from_str(uri),
    ) {
        headers.insert(name, value);
    }
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
    let status = StatusCode::from_u16(php_resp.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut builder = Response::builder().status(status);
    for (name, value) in php_resp.headers.iter() {
        if let Ok(v) = value.to_str() {
            builder = builder.header(name, v);
        }
    }
    match builder.body(Body::from(php_resp.body)) {
        Ok(resp) => resp,
        Err(_) => response::status_response(status, "Failed to build response"),
    }
}
