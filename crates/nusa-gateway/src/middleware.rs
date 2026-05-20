//! Tenant extraction middleware for multi-tenant isolation.
//!
//! Skills applied:
//! - `m05-type-driven`: TenantId newtype prevents context mixing
//! - `m09-domain`: Tenant isolation enforced at gateway level
//! - `domain-web`: Header-based tenant extraction for SaaS routing

use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::Response,
    middleware::Next,
};

use nusa_core::TenantId;

/// Extract tenant ID from headers.
pub fn extract_tenant(headers: &http::HeaderMap) -> Option<TenantId> {
    if let Some(header) = headers.get("x-tenant-id")
        && let Ok(value) = header.to_str() {
        return Some(TenantId::new(value));
    }

    if let Some(host) = headers.get("host")
        && let Ok(host_str) = host.to_str()
        && let Some(subdomain) = host_str.split('.').next()
        && !subdomain.is_empty()
        && subdomain != "localhost"
        && subdomain != "127.0.0.1" {
        return Some(TenantId::new(subdomain));
    }

    None
}

/// Extract W3C TraceContext trace ID from headers.
pub fn extract_trace_id(headers: &http::HeaderMap) -> nusa_core::TraceId {
    if let Some(traceparent) = headers.get("traceparent")
        && let Ok(value) = traceparent.to_str() {
        let parts: Vec<&str> = value.split('-').collect();
        if parts.len() >= 3 && parts[1].len() == 32 {
            let hex_str = parts[1];
            if let (Ok(high), Ok(low)) = (
                u128::from_str_radix(&hex_str[..16], 16),
                u128::from_str_radix(&hex_str[16..], 16),
            ) {
                let mut bytes = [0u8; 16];
                bytes[..8].copy_from_slice(&high.to_be_bytes());
                bytes[8..].copy_from_slice(&low.to_be_bytes());
                if let Ok(uuid) = uuid::Uuid::from_slice(&bytes) {
                    return nusa_core::TraceId::from_uuid(uuid);
                }
            }
        }
    }
    nusa_core::TraceId::new()
}

/// Request size limit middleware (m13-domain-error: reject oversized requests)
pub async fn request_size_limit(
    req: Request<Body>,
    next: Next,
) -> Response<Body> {
    if let Some(cl) = req.headers().get(http::header::CONTENT_LENGTH)
        && let Ok(s) = cl.to_str()
        && let Ok(len) = s.parse::<usize>()
        && len > 10 * 1024 * 1024 {
        return Response::builder()
            .status(StatusCode::PAYLOAD_TOO_LARGE)
            .body(Body::from("Request too large"))
            .expect("builder with valid status always succeeds");
    }
    next.run(req).await
}
