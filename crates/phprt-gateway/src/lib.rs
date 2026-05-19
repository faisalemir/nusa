#![deny(unsafe_code)]
#![warn(clippy::all)]

pub mod circuit_breaker;
pub mod health;
pub mod tls;

use std::sync::Arc;
use std::time::Duration;

use axum::{
    Router,
    body::Body,
    extract::State,
    http::{Request, StatusCode, HeaderMap},
    response::Response,
};
use tower::ServiceBuilder;
use tower_http::{
    trace::TraceLayer,
    cors::CorsLayer,
};
use tracing::{error, info_span, warn};

use phprt_core::{
    PhpEngine, RequestContext, PhpResponse, TenantId, TraceId,
    BackpressureGuard, ResourceGuard, with_timeout,
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
);

/// Main application router (domain-web + m07-concurrency)
pub fn app(
    engine: Arc<dyn PhpEngine>,
    plugins: Arc<PluginRegistry>,
    circuit_breaker: Arc<CircuitBreaker>,
    health_state: Arc<HealthState>,
    backpressure: Arc<BackpressureGuard>,
    resource_guard: ResourceGuard,
) -> Router {
    Router::new()
        // Health & Readiness Probes
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
        // Catch-all route for PHP scripts
        .route("/{*path}", axum::routing::get(handler).post(handler))
        // Middleware Chain (domain-web)
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive())
        )
        .with_state((engine, plugins, circuit_breaker, health_state, backpressure, resource_guard))
}

/// Extract tenant from request headers.
fn extract_tenant(headers: &HeaderMap) -> Option<TenantId> {
    if let Some(header) = headers.get("x-tenant-id")
        && let Ok(value) = header.to_str() {
            return Some(TenantId::new(value));
        }
    // Try to extract from host header (subdomain)
    if let Some(host) = headers.get("host")
        && let Ok(host_str) = host.to_str()
            && let Some(subdomain) = host_str.split('.').next()
                && !subdomain.is_empty() && subdomain != "localhost" && subdomain != "127.0.0.1" {
                    return Some(TenantId::new(subdomain));
                }
    None
}

/// Extract trace ID from W3C TraceContext header.
fn extract_trace_id(headers: &HeaderMap) -> TraceId {
    if let Some(traceparent) = headers.get("traceparent")
        && let Ok(value) = traceparent.to_str() {
            // W3C TraceContext format: version-traceId-parentId-sampled
            // 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01
            let parts: Vec<&str> = value.split('-').collect();
            if parts.len() >= 3 {
                // Parse trace ID hex string directly (32 hex chars = 16 bytes)
                let hex_str = parts[1];
                if hex_str.len() == 32 {
                    let mut bytes = [0u8; 16];
                    for i in 0..16 {
                        if let Ok(byte_val) = u8::from_str_radix(&hex_str[i*2..(i+1)*2], 16) {
                            bytes[i] = byte_val;
                        }
                    }
                    if let Ok(uuid) = uuid::Uuid::from_slice(&bytes) {
                        return TraceId::from_uuid(uuid);
                    }
                }
            }
        }
    TraceId::new()
}

/// Build RequestContext from HTTP request.
///
/// `domain-web`: HTTP → domain model conversion.
async fn build_request_context(
    req: Request<Body>,
    guard: &ResourceGuard,
) -> Result<RequestContext, StatusCode> {
    // Validate content length before reading body
    let content_length = req.headers()
        .get(http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    if !phprt_core::validate_request_size(content_length, guard.max_request_bytes) {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }

    // Extract headers and tenant before consuming the request
    let headers = req.headers().clone();
    let tenant = extract_tenant(req.headers());

    // Read body with size limit (consumes the request)
    let body_bytes = axum::body::to_bytes(req.into_body(), guard.max_request_bytes)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    // Build RequestContext
    let mut ctx = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_millis(guard.request_timeout_ms),
    )
    .with_body(body_bytes)
    .with_headers(headers)
    .with_env(Arc::new(std::env::vars().collect()));

    // Add tenant if present
    if let Some(t) = tenant {
        ctx = ctx.with_tenant(t);
    }

    Ok(ctx)
}

/// Convert PhpResponse to HTTP Response.
///
/// `domain-web`: domain → HTTP response conversion.
fn build_http_response(php_resp: PhpResponse) -> Response<Body> {
    let mut resp = Response::builder()
        .status(php_resp.status)
        .body(Body::from(php_resp.body.to_vec()))
        .expect("builder with valid status always succeeds");
    *resp.headers_mut() = php_resp.headers;
    resp
}

/// Request Handler (m13-domain-error: EngineError -> HTTP status)
async fn handler(
    State((engine, _plugins, cb, health, bp, guard)): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let trace_id = extract_trace_id(req.headers());

    let span = info_span!("request", method = %method, uri = %uri, trace_id = %trace_id);
    let _enter = span.enter();

    // Check circuit breaker (m13-domain-error)
    if !cb.allow_request() {
        warn!("circuit breaker open, rejecting request");
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
            warn!("backpressure limit reached, rejecting request");
            health.record_error();
            return Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .body(Body::from("Service Unavailable: Too Many Requests"))
                .expect("builder with valid status always succeeds");
        }
    };

    // Build request context (consumes req)
    let ctx = match build_request_context(req, &guard).await {
        Ok(ctx) => ctx,
        Err(status) => {
            return Response::builder()
                .status(status)
                .body(Body::from("Bad Request"))
                .expect("builder with valid status always succeeds");
        }
    };

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
            error!(err = %e, "engine execution failed");
            let status = e.to_http_status();
            Response::builder()
                .status(status)
                .body(Body::from(format!("Upstream Error: {e}")))
                .expect("builder with valid status always succeeds")
        }
    }
}
