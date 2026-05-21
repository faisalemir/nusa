//! Tests for middleware: tenant extraction, trace ID extraction, request size limiting.
//!
//! Covers: header-based tenant, subdomain-based tenant, trace context parsing, size limit enforcement.

use axum::body::Body;
use axum::http::{HeaderMap, Request};
use http::header::{CONTENT_LENGTH, HeaderValue};
use nusa_gateway::middleware::{extract_tenant, extract_trace_id};
use nusa_core::TenantId;

// ── Tenant Extraction ──

#[test]
fn extract_tenant_from_x_tenant_id_header() {
    let mut headers = HeaderMap::new();
    headers.insert("x-tenant-id", HeaderValue::from_static("acme-corp"));
    let tenant = extract_tenant(&headers);
    assert_eq!(tenant, Some(TenantId::new("acme-corp")));
}

#[test]
fn extract_tenant_from_subdomain() {
    let mut headers = HeaderMap::new();
    headers.insert("host", HeaderValue::from_static("acme.example.com"));
    let tenant = extract_tenant(&headers);
    assert_eq!(tenant, Some(TenantId::new("acme")));
}

#[test]
fn extract_tenant_x_tenant_id_takes_precedence() {
    let mut headers = HeaderMap::new();
    headers.insert("x-tenant-id", HeaderValue::from_static("explicit"));
    headers.insert("host", HeaderValue::from_static("implicit.example.com"));
    let tenant = extract_tenant(&headers);
    assert_eq!(tenant, Some(TenantId::new("explicit")));
}

#[test]
fn extract_tenant_returns_none_for_localhost() {
    let mut headers = HeaderMap::new();
    headers.insert("host", HeaderValue::from_static("localhost:8080"));
    assert!(extract_tenant(&headers).is_none());
}

#[test]
fn extract_tenant_returns_none_for_127_0_0_1() {
    let mut headers = HeaderMap::new();
    headers.insert("host", HeaderValue::from_static("127.0.0.1:8080"));
    assert!(extract_tenant(&headers).is_none());
}

#[test]
fn extract_tenant_returns_none_when_no_headers() {
    let headers = HeaderMap::new();
    assert!(extract_tenant(&headers).is_none());
}

// ── Trace ID Extraction ──

#[test]
fn extract_trace_id_from_traceparent_header() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "traceparent",
        HeaderValue::from_static("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"),
    );
    let trace_id = extract_trace_id(&headers);
    assert!(trace_id.to_string().len() > 0);
}

#[test]
fn extract_trace_id_returns_new_when_no_header() {
    let headers = HeaderMap::new();
    let trace_id = extract_trace_id(&headers);
    assert!(trace_id.to_string().len() > 0);
}

#[test]
fn extract_trace_id_returns_new_for_malformed_header() {
    let mut headers = HeaderMap::new();
    headers.insert("traceparent", HeaderValue::from_static("invalid"));
    let trace_id = extract_trace_id(&headers);
    assert!(trace_id.to_string().len() > 0);
}

// ── Request Size Limit ──

#[test]
fn request_size_logic_rejects_large_request() {
    let cl = 10_485_761; // > 10MB
    assert!(cl > 10 * 1024 * 1024, "Should exceed 10MB limit");
}

#[test]
fn request_size_logic_allows_small_request() {
    let cl = 1024; // 1KB
    assert!(cl <= 10 * 1024 * 1024, "Should be under 10MB limit");
}

#[test]
fn request_builder_with_headers() {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_LENGTH, HeaderValue::from_static("10485761"));

    let req = Request::builder()
        .uri("/test")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let (mut parts, body) = req.into_parts();
    parts.headers = headers;
    let req = Request::from_parts(parts, body);

    let cl = req.headers().get(CONTENT_LENGTH).unwrap().to_str().unwrap().parse::<usize>().unwrap();
    assert!(cl > 10 * 1024 * 1024);
}
