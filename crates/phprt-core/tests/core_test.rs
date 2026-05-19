//! Unit tests for phprt-core
//!
//! Skills applied:
//! - `coding-guidelines`: assert! for invariants, no unwrap() in lib code
//! - `m06-error-handling`: typed errors via thiserror, ErrorKind mapping
//! - `m05-type-driven`: newtype wrappers, encapsulation
//! - `m01-ownership`: Clone/Copy semantics, Arc sharing

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::HeaderMap;
use phprt_core::{
    EngineError, PhpEngine, PhpResponse, RequestContext,
    TenantId, TraceId, WorkerId,
};

// ── TraceId: newtype wrapper (m05-type-driven) ──

#[test]
fn trace_id_generates_unique_ids() {
    let a = TraceId::new();
    let b = TraceId::new();
    assert_ne!(a, b, "two TraceId calls must produce unique IDs");
}

#[test]
fn trace_id_display_uuid_format() {
    let id = TraceId::new();
    let s = id.to_string();
    // UUID v4: 36 chars (8-4-4-4-12 with hyphens)
    assert_eq!(s.len(), 36, "TraceId must display as 36-char UUID");
}

#[test]
fn trace_id_default_generates_valid_id() {
    let a = TraceId::default();
    let b = TraceId::new();
    assert_ne!(a, b, "Default must also generate a unique ID");
}

#[test]
fn trace_id_as_uuid_roundtrip() {
    let id = TraceId::new();
    let uuid = id.as_uuid();
    assert_eq!(uuid.to_string(), id.to_string());
}

// ── TenantId: encapsulated newtype ──

#[test]
fn tenant_id_encapsulation() {
    let t = TenantId::new("acme-corp");
    assert_eq!(t.as_str(), "acme-corp");
    assert_eq!(t.to_string(), "acme-corp");
}

#[test]
fn tenant_id_from_string_owned() {
    let name = String::from("tenant-42");
    let t = TenantId::new(name.clone());
    assert_eq!(t.as_str(), "tenant-42");
    // Original string is consumed
    drop(name);
}

// ── WorkerId: value type ──

#[test]
fn worker_id_display() {
    let w = WorkerId::new(42);
    assert_eq!(w.to_string(), "worker-42");
}

#[test]
fn worker_id_as_usize() {
    let w = WorkerId::new(7);
    assert_eq!(w.as_usize(), 7);
}

// ── RequestContext: builder pattern, immutable after construction ──

#[test]
fn request_context_builder_defaults() {
    let ctx = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );

    assert_eq!(ctx.vfs_root().to_str(), Some("/app/public"));
    assert_eq!(ctx.script_path().to_str(), Some("index.php"));
    assert!(ctx.tenant_id().is_none(), "tenant_id defaults to None");
}

#[test]
fn request_context_with_tenant() {
    let ctx = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_tenant(TenantId::new("tenant-1"));

    assert!(ctx.tenant_id().is_some());
    assert_eq!(ctx.tenant_id().unwrap().as_str(), "tenant-1");
}

#[test]
fn request_context_with_body() {
    let body = Bytes::from("hello world");
    let ctx = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_body(body.clone());

    assert_eq!(ctx.body(), &body);
}

#[test]
fn request_context_with_headers() {
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse().expect("valid header value"));

    let ctx = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_headers(headers.clone());

    assert_eq!(ctx.headers(), &headers);
}

#[test]
fn request_context_with_env() {
    let env = Arc::new(HashMap::from([
        ("APP_ENV".to_string(), "testing".to_string()),
    ]));

    let ctx = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_env(env.clone());

    assert_eq!(*ctx.env(), env);
}

#[test]
fn request_context_clone_shares_env_arc() {
    let env = Arc::new(HashMap::new());
    let ctx = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_env(env);

    let ctx2 = ctx.clone();
    // m01-ownership: both share the same Arc (cheap clone)
    assert!(Arc::ptr_eq(ctx.env(), ctx2.env()));
}

// ── EngineError: typed errors (m06-error-handling) ──

#[test]
fn engine_error_to_http_status_mapping() {
    // coding-guidelines: assert! for invariant checking
    assert_eq!(EngineError::Timeout.to_http_status(), 408);
    assert_eq!(EngineError::Sandbox("test".into()).to_http_status(), 500);
    assert_eq!(EngineError::PhpFatal("err".into()).to_http_status(), 502);
    assert_eq!(EngineError::ResourceLimit.to_http_status(), 429);
    assert_eq!(EngineError::Plugin("err".into()).to_http_status(), 500);
    assert_eq!(EngineError::IpcProtocol("err".into()).to_http_status(), 500);
}

#[test]
fn engine_error_display() {
    let e = EngineError::Timeout;
    let s = e.to_string();
    assert_eq!(s, "execution timeout");
}

#[test]
fn engine_error_sandbox_includes_message() {
    let e = EngineError::Sandbox("path traversal blocked".into());
    let s = e.to_string();
    assert!(s.contains("path traversal blocked"));
}

// ── PhpResponse: value type ──

#[test]
fn php_response_basic() {
    let resp = PhpResponse {
        status: 200,
        headers: HeaderMap::new(),
        body: Bytes::from("hello"),
    };
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, Bytes::from("hello"));
}

#[test]
fn php_response_clone() {
    let resp = PhpResponse {
        status: 201,
        headers: HeaderMap::new(),
        body: Bytes::from("created"),
    };
    let resp2 = resp.clone();
    assert_eq!(resp2.status, 201);
}

// ── Trait object safety (m04-zero-cost) ──

#[test]
fn php_engine_is_object_safe() {
    // This test verifies PhpEngine can be used as a trait object.
    // If this compiles, the trait is object-safe.
    fn _assert_object_safe(_: Arc<dyn PhpEngine>) {}
}
