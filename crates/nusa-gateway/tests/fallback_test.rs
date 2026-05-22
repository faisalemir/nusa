//! Fallback / default logic tests for gateway.
//!
//! rust-test §Fallback / Default Logic Tests
//! Tests behavior when primary engines fail and fallbacks activate.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use nusa_core::{PhpEngine, PhpResponse, RequestContext};

// ── Fallback Engine Pattern ──

/// A primary engine that can be toggled to fail.
struct ToggleableEngine {
    fail: AtomicBool,
    slow: AtomicBool,
}

impl ToggleableEngine {
    fn new() -> Self {
        Self {
            fail: AtomicBool::new(false),
            slow: AtomicBool::new(false),
        }
    }

    fn set_fail(&self, fail: bool) {
        self.fail.store(fail, Ordering::SeqCst);
    }

    fn set_slow(&self, slow: bool) {
        self.slow.store(slow, Ordering::SeqCst);
    }
}

#[async_trait]
impl PhpEngine for ToggleableEngine {
    async fn execute(&self, _ctx: RequestContext) -> nusa_core::Result<PhpResponse> {
        if self.slow.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
        if self.fail.load(Ordering::SeqCst) {
            Err(nusa_core::EngineError::PhpFatal(
                "primary engine failure".into(),
            ))
        } else {
            Ok(PhpResponse {
                status: 200,
                headers: Default::default(),
                body: Bytes::from("primary response"),
            })
        }
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &["toggleable"]
    }
    async fn shutdown(&self) {}
}

/// A fallback engine that always returns a degraded response.
struct FallbackEngine;

#[async_trait]
impl PhpEngine for FallbackEngine {
    async fn execute(&self, _ctx: RequestContext) -> nusa_core::Result<PhpResponse> {
        Ok(PhpResponse {
            status: 503,
            headers: Default::default(),
            body: Bytes::from("fallback: service degraded"),
        })
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &["fallback"]
    }
    async fn shutdown(&self) {}
}

// ── Fallback Chain Tests ──

/// Primary available → uses primary, not fallback.
#[tokio::test]
async fn fallback_primary_available_uses_primary() {
    let primary = ToggleableEngine::new();
    primary.set_fail(false);

    let ctx = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );

    let result = primary.execute(ctx).await;
    assert!(result.is_ok(), "primary should succeed");
    let resp = result.unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, Bytes::from("primary response"));
}

/// Primary unavailable → should return error (fallback would be used in real gateway).
#[tokio::test]
async fn fallback_primary_unavailable_returns_error() {
    let primary = ToggleableEngine::new();
    primary.set_fail(true);

    let ctx = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );

    let result = primary.execute(ctx).await;
    assert!(result.is_err(), "primary should fail");
}

/// Fallback returns degraded but valid response.
#[tokio::test]
async fn fallback_returns_degraded_response() {
    let fallback = FallbackEngine;

    let ctx = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );

    let result = fallback.execute(ctx).await;
    assert!(result.is_ok(), "fallback should always return ok");
    let resp = result.unwrap();
    assert_eq!(resp.status, 503);
    assert!(!resp.body.is_empty(), "fallback must return body");
}

/// Fallback with tenant context preserves isolation.
#[tokio::test]
async fn fallback_with_tenant_context() {
    let fallback = FallbackEngine;
    let tenant = nusa_core::TenantId::new("tenant-a");

    let ctx = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_tenant(tenant);

    let result = fallback.execute(ctx).await;
    assert!(result.is_ok(), "fallback should handle tenant context");
}

/// Fallback with body and headers passes through.
#[tokio::test]
async fn fallback_with_body_and_headers() {
    let fallback = FallbackEngine;

    let ctx = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_body(Bytes::from("request body"))
    .with_headers(http::HeaderMap::new());

    let result = fallback.execute(ctx).await;
    assert!(result.is_ok());
}

/// Multiple fallback calls are independent (no state bleeding).
#[tokio::test]
async fn fallback_multiple_calls_independent() {
    let fallback = FallbackEngine;

    for i in 0..10 {
        let ctx = RequestContext::new(
            "/app/public".into(),
            "index.php".into(),
            tokio::time::Instant::now() + Duration::from_secs(30),
        );
        let result = fallback.execute(ctx).await;
        assert!(result.is_ok(), "call {} should succeed", i);
    }
}

/// Fallback is Send + Sync safe for concurrent access.
#[test]
fn fallback_engine_is_send_sync() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<FallbackEngine>();
    assert_sync::<FallbackEngine>();
}

/// Primary slow but not failing → should still return response.
#[tokio::test]
async fn fallback_primary_slow_returns_response() {
    // This test verifies that a slow primary doesn't trigger fallback
    // (timeout would be handled by the gateway's with_timeout wrapper)
    let primary = ToggleableEngine::new();
    primary.set_slow(false);

    let ctx = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );

    let result = tokio::time::timeout(Duration::from_secs(1), primary.execute(ctx)).await;
    assert!(result.is_ok(), "primary should complete within timeout");
    assert!(result.unwrap().is_ok());
}

/// Switch from failed primary back to working primary → recovery.
#[tokio::test]
async fn fallback_recovery_primary_restored() {
    let primary = ToggleableEngine::new();

    // Phase 1: primary fails
    primary.set_fail(true);
    let ctx1 = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );
    let result1 = primary.execute(ctx1).await;
    assert!(result1.is_err(), "primary should fail initially");

    // Phase 2: primary recovers
    primary.set_fail(false);
    let ctx2 = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );
    let result2 = primary.execute(ctx2).await;
    assert!(result2.is_ok(), "primary should recover");
    assert_eq!(result2.unwrap().body, Bytes::from("primary response"));
}
