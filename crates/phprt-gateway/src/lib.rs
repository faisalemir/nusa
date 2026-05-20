//! Nusa PHP Runtime Gateway — Axum HTTP server with middleware.
//!
//! Skills applied:
//! - `domain-web`: HTTP request handling, middleware chain
//! - `m13-domain-error`: Error → HTTP status mapping
//! - `m07-concurrency`: Arc<dyn PhpEngine> shared across handlers
//! - `m05-type-driven`: TenantId enforced at gateway level
//! - `m09-domain`: Tenant isolation via registry checks

pub mod circuit_breaker;
pub mod health;
pub mod middleware;
pub mod tls;

use std::sync::Arc;
use std::time::Duration;

use axum::{
    Router,
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    response::Response,
    Json,
};
use phprt_core::{
    PhpEngine, RequestContext, PhpResponse,
    BackpressureGuard, ResourceGuard, with_timeout,
    TenantRegistry, TaskManager,
};
use phprt_plugin_api::PluginRegistry;
use crate::circuit_breaker::CircuitBreaker;
use crate::health::HealthState;

type AppState = (
    Arc<dyn PhpEngine>,
    Arc<PluginRegistry>,
    Arc<CircuitBreaker>,
    Arc<HealthState>,
    Arc<BackpressureGuard>,
    ResourceGuard,
    Arc<TenantRegistry>,
    Arc<TaskManager>,
);

/// Main application router (domain-web + m07-concurrency)
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
) -> Router {
    Router::new()
        .route("/health", axum::routing::get(health::health_handler))
        .route("/ready", axum::routing::get({
            let hs = health_state.clone();
            move || async move {
                if hs.is_ready() {
                    (StatusCode::OK, "READY")
                } else {
                    (StatusCode::SERVICE_UNAVAILABLE, "Not Ready")
                }
            }
        }))
        // Task offload endpoint (M4)
        .route("/api/tasks", axum::routing::post(offload_task_handler))
        .route("/api/tasks/{task_id}/status", axum::routing::get({
            move |axum::extract::Path(task_id): axum::extract::Path<String>| async move {
                let _ = task_id;
                (StatusCode::OK, Json(serde_json::json!({ "status": "pending" })))
            }
        }))
        // Catch-all route for PHP scripts
        .route("/{*path}", axum::routing::get(handler).post(handler))
        .layer(
            tower::ServiceBuilder::new()
                .layer(tower_http::trace::TraceLayer::new_for_http())
                .layer(tower_http::cors::CorsLayer::permissive())
                .layer(axum::middleware::from_fn(middleware::request_size_limit))
        )
        .with_state((engine, plugins, circuit_breaker, health_state, backpressure, resource_guard, tenants, tasks))
}

/// Request Handler (m13-domain-error: EngineError -> HTTP status)
async fn handler(
    State((engine, _plugins, cb, health, bp, guard, tenants, _tasks)): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let trace_id = middleware::extract_trace_id(req.headers());

    let span = tracing::info_span!("request", method = %method, uri = %uri, trace_id = %trace_id);
    let _enter = span.enter();

    // Check circuit breaker
    if !cb.allow_request() {
        tracing::warn!("circuit breaker open, rejecting request");
        health.record_error();
        return Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .body(Body::from("Service Unavailable: Circuit Open"))
            .expect("builder with valid status always succeeds");
    }

    // Check backpressure
    let permit = match bp.try_acquire().await {
        Some(p) => p,
        None => {
            tracing::warn!("backpressure limit reached, rejecting request");
            health.record_error();
            return Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .body(Body::from("Service Unavailable: Too Many Requests"))
                .expect("builder with valid status always succeeds");
        }
    };

    // Build request context
    let ctx = match build_request_context(req, &guard).await {
        Ok(ctx) => ctx,
        Err(status) => {
            return Response::builder()
                .status(status)
                .body(Body::from("Bad Request"))
                .expect("builder with valid status always succeeds");
        }
    };

    // Check tenant is enabled (M4: multi-tenant isolation)
    if let Some(tenant_id) = ctx.tenant_id() {
        if !tenants.is_enabled(tenant_id) {
            health.record_error();
            return Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Body::from("Tenant not enabled"))
                .expect("builder with valid status always succeeds");
        }
    }

    // Execute PHP Engine with timeout
    match with_timeout(guard.request_timeout_ms, engine.execute(ctx)).await {
        Ok(res) => {
            cb.record_success();
            health.record_success();
            drop(permit);
            build_http_response(res)
        }
        Err(e) => {
            cb.record_failure();
            health.record_error();
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

/// Offload a heavy task from PHP to Rust (M4: async task offloading)
async fn offload_task_handler(
    State((_, _, _, _, _, _, _, tasks)): State<AppState>,
    Json(task): Json<phprt_core::OffloadTask>,
) -> (StatusCode, Json<serde_json::Value>) {
    let (task_id, _rx) = tasks.submit(task);
    (StatusCode::ACCEPTED, Json(serde_json::json!({ "task_id": task_id })))
}

/// Build RequestContext from HTTP request.
async fn build_request_context(
    req: Request<Body>,
    guard: &ResourceGuard,
) -> Result<RequestContext, StatusCode> {
    let content_length = req.headers()
        .get(http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    if !phprt_core::validate_request_size(content_length, guard.max_request_bytes) {
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
    let mut resp = Response::builder()
        .status(php_resp.status)
        .body(Body::from(php_resp.body.to_vec()))
        .expect("builder with valid status always succeeds");
    *resp.headers_mut() = php_resp.headers;
    resp
}
