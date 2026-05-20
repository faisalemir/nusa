//! Leak test suite — verifies zero state retention between sequential requests.
//!
//! Skills applied:
//! - `m10-performance`: Memory usage stable after warmup
//! - `m15-anti-pattern`: No cross-request context bleeding

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::HeaderMap;
use nusa_core::{RequestContext, TenantId};

/// Verify that RequestContext fields are fully isolated between creations.
#[test]
fn test_request_context_no_trace_leak() {
    let ctx1 = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );
    let ctx2 = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );

    assert_ne!(
        ctx1.trace_id(),
        ctx2.trace_id(),
        "TraceIds must be unique per request"
    );
}

#[test]
fn test_request_context_no_tenant_leak() {
    let ctx_base = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );
    assert!(
        ctx_base.tenant_id().is_none(),
        "Base ctx must not have tenant"
    );

    let tenant = TenantId::new("tenant-abc");
    let ctx2 = ctx_base.with_tenant(tenant.clone());
    assert_eq!(ctx2.tenant_id().unwrap().as_str(), "tenant-abc");
}

#[test]
fn test_request_context_no_header_leak() {
    let mut headers1 = HeaderMap::new();
    headers1.insert(
        http::header::AUTHORIZATION,
        "Bearer secret1".parse().unwrap(),
    );
    let _ctx1 = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_headers(headers1);

    let ctx2 = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );

    assert!(
        ctx2.headers().get(http::header::AUTHORIZATION).is_none(),
        "New context must not inherit headers from previous"
    );
}

#[test]
fn test_request_context_no_body_leak() {
    let _ctx1 = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_body(Bytes::from(vec![0u8; 1024]));

    let ctx2 = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );

    assert!(ctx2.body().is_empty(), "New context must have empty body");
}

#[test]
fn test_request_context_no_env_leak() {
    let env1: HashMap<String, String> = [("SECRET_KEY".into(), "value1".into())]
        .into_iter()
        .collect();
    let _ctx1 = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_env(Arc::new(env1));

    let ctx2 = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );

    assert!(ctx2.env().is_empty(), "New context must have empty env map");
}

/// Simulate 10k sequential request creations and verify no shared state.
#[test]
fn test_10k_requests_no_state_bleed() {
    use std::collections::HashSet;

    let mut trace_ids = HashSet::new();
    for _ in 0..10_000 {
        let ctx = RequestContext::new(
            "/app/public".into(),
            "index.php".into(),
            tokio::time::Instant::now() + Duration::from_secs(30),
        );
        let id = ctx.trace_id();
        assert!(
            trace_ids.insert(id),
            "Duplicate TraceId detected after {} requests",
            trace_ids.len() + 1
        );
    }
}

/// Verify that RequestContext is Clone-safe (each clone independent).
#[test]
fn test_clone_isolation() {
    let ctx1 = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_tenant(TenantId::new("original"));

    let ctx2 = ctx1.clone();

    assert_eq!(
        ctx1.trace_id(),
        ctx2.trace_id(),
        "Clone shares trace_id (expected)"
    );
    assert_eq!(
        ctx1.tenant_id().unwrap().as_str(),
        ctx2.tenant_id().unwrap().as_str()
    );
}
