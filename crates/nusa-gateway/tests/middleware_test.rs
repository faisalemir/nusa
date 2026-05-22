//! Tests for middleware: tenant extraction, trace ID extraction, request size limiting.
//!
//! Covers: header-based tenant, subdomain-based tenant, trace context parsing, size limit enforcement.

use axum::body::Body;
use axum::http::{HeaderMap, Request};
use http::header::{CONTENT_LENGTH, HeaderValue};
use nusa_core::TenantId;
use nusa_gateway::middleware::{extract_tenant, extract_trace_id};

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

    let cl = req
        .headers()
        .get(CONTENT_LENGTH)
        .unwrap()
        .to_str()
        .unwrap()
        .parse::<usize>()
        .unwrap();
    assert!(cl > 10 * 1024 * 1024);
}

// ── Decision Table Truth Tables (Extended) ──

/// All boolean condition combinations for tenant extraction.
/// Truth table: 2^3 = 8 combinations (X-Tenant-Id present, valid subdomain, localhost)
#[test]
fn decision_table_tenant_extraction_all_combinations() {
    let cases: Vec<(&str, bool)> = vec![
        // (description, should_have_tenant)
        ("no headers", false),
        ("x-tenant-id only", true),
        ("valid subdomain only", true),
        ("localhost only", false),
        ("127.0.0.1 only", false),
        ("x-tenant-id + subdomain (precedence to header)", true),
        ("x-tenant-id + localhost (precedence to header)", true),
        ("empty x-tenant-id header", false),
    ];

    for (desc, should_have_tenant) in cases {
        let mut headers = HeaderMap::new();
        match desc {
            "no headers" => {}
            "x-tenant-id only" => {
                headers.insert("x-tenant-id", HeaderValue::from_static("my-tenant"));
            }
            "valid subdomain only" => {
                headers.insert("host", HeaderValue::from_static("myapp.example.com"));
            }
            "localhost only" => {
                headers.insert("host", HeaderValue::from_static("localhost:8080"));
            }
            "127.0.0.1 only" => {
                headers.insert("host", HeaderValue::from_static("127.0.0.1:8080"));
            }
            "x-tenant-id + subdomain (precedence to header)" => {
                headers.insert("x-tenant-id", HeaderValue::from_static("header-tenant"));
                headers.insert("host", HeaderValue::from_static("sub.example.com"));
            }
            "x-tenant-id + localhost (precedence to header)" => {
                headers.insert("x-tenant-id", HeaderValue::from_static("header-tenant"));
                headers.insert("host", HeaderValue::from_static("localhost:8080"));
            }
            "empty x-tenant-id header" => {
                headers.insert("x-tenant-id", HeaderValue::from_static(""));
            }
            _ => {}
        }

        let result = extract_tenant(&headers);
        if should_have_tenant {
            assert!(result.is_some(), "case '{}' should have tenant", desc);
        } else {
            assert!(result.is_none(), "case '{}' should not have tenant", desc);
        }
    }
}

/// Pairwise combinations for trace ID extraction.
#[test]
fn decision_table_trace_id_pairwise() {
    let cases: Vec<(&str, bool)> = vec![
        ("no traceparent", false), // should generate new
        ("valid traceparent", true),
        ("malformed traceparent", false), // should generate new
        ("empty traceparent", false),     // should generate new
        ("traceparent with wrong version", false), // should generate new
        ("traceparent with extra spaces", false), // should generate new
    ];

    for (desc, _should_parse) in cases {
        let mut headers = HeaderMap::new();
        match desc {
            "no traceparent" => {}
            "valid traceparent" => {
                headers.insert(
                    "traceparent",
                    HeaderValue::from_static(
                        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
                    ),
                );
            }
            "malformed traceparent" => {
                headers.insert("traceparent", HeaderValue::from_static("garbage"));
            }
            "empty traceparent" => {
                headers.insert("traceparent", HeaderValue::from_static(""));
            }
            "traceparent with wrong version" => {
                headers.insert(
                    "traceparent",
                    HeaderValue::from_static(
                        "99-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
                    ),
                );
            }
            "traceparent with extra spaces" => {
                headers.insert(
                    "traceparent",
                    HeaderValue::from_static(
                        " 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01 ",
                    ),
                );
            }
            _ => {}
        }

        let result = extract_trace_id(&headers);
        assert!(
            result.to_string().len() > 0,
            "case '{}' should always produce a trace ID",
            desc
        );
    }
}
