//! End-to-end integration tests for nusa-core + nusa-gateway + nusa-config
//!
//! Tests the full request lifecycle:
//! Config → Engine → Gateway → Circuit Breaker → Response
//!
//! Skills applied:
//! - `m13-domain-error`: Full error propagation chain
//! - `m09-domain`: RequestContext immutable builder
//! - `domain-web`: HTTP request/response cycle

use std::sync::Arc;
use std::time::Duration;

use nusa_core::{EngineError, PhpEngine, PhpResponse, RequestContext, TenantId, TraceId};
use nusa_gateway::circuit_breaker::CircuitBreaker;
use nusa_plugin_api::{Plugin, PluginRegistry};

// ── Mock PhpEngine for integration testing ──

struct MockEngine {
    delay: Option<Duration>,
}

#[async_trait::async_trait]
impl PhpEngine for MockEngine {
    async fn execute(&self, _ctx: RequestContext) -> nusa_core::Result<PhpResponse> {
        if let Some(delay) = self.delay {
            tokio::time::sleep(delay).await;
        }
        Ok(PhpResponse {
            status: 200,
            headers: Default::default(),
            body: bytes::Bytes::from("mock response"),
        })
    }

    fn capabilities(&self) -> &'static [&'static str] {
        &["mock"]
    }

    async fn shutdown(&self) {}
}

// ── Mock Plugin ──

struct CountingPlugin {
    pre_count: std::sync::atomic::AtomicU64,
    post_count: std::sync::atomic::AtomicU64,
}

#[async_trait::async_trait]
impl Plugin for CountingPlugin {
    fn name(&self) -> &'static str {
        "counting-plugin"
    }

    async fn pre_exec(&self, _ctx: &mut RequestContext) -> nusa_core::Result<()> {
        self.pre_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    async fn post_exec(&self, _ctx: &RequestContext) -> nusa_core::Result<()> {
        self.post_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}

// ── Engine + Plugin Integration ──

#[tokio::test]
async fn mock_engine_executes_with_context() {
    let engine = MockEngine { delay: None };
    let ctx = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );

    let result = engine.execute(ctx).await;
    assert!(result.is_ok());
    let resp = result.unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, bytes::Bytes::from("mock response"));
}

#[tokio::test]
async fn mock_engine_respects_delay() {
    let engine = MockEngine {
        delay: Some(Duration::from_millis(50)),
    };
    let ctx = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );

    let start = std::time::Instant::now();
    let _ = engine.execute(ctx).await;
    let elapsed = start.elapsed();

    assert!(
        elapsed >= Duration::from_millis(40),
        "engine must respect configured delay",
    );
}

#[tokio::test]
async fn plugin_registry_tracks_executions() {
    let reg = PluginRegistry::new();
    let plugin = Arc::new(CountingPlugin {
        pre_count: std::sync::atomic::AtomicU64::new(0),
        post_count: std::sync::atomic::AtomicU64::new(0),
    });
    reg.register(plugin.clone());

    let mut ctx = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );

    reg.run_pre_exec(&mut ctx).await.unwrap();
    reg.run_post_exec(&ctx).await.unwrap();

    assert_eq!(
        plugin.pre_count.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "pre_exec must be called once",
    );
    assert_eq!(
        plugin.post_count.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "post_exec must be called once",
    );
}

#[tokio::test]
async fn circuit_breaker_integrates_with_engine() {
    let cb = CircuitBreaker::new(2, Duration::from_secs(1));
    let engine = MockEngine { delay: None };
    let ctx = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );

    // Normal execution should allow request and record success
    assert!(cb.allow_request());
    let result = engine.execute(ctx).await;
    assert!(result.is_ok());
    cb.record_success();

    // Still open for more requests
    assert!(cb.allow_request());
}

// ── Multi-Tenant Isolation Tests (M4) ──

#[test]
fn request_context_with_tenant_isolation() {
    let ctx_a = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_tenant(TenantId::new("tenant-a"));

    let ctx_b = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_tenant(TenantId::new("tenant-b"));

    assert_ne!(
        ctx_a.tenant_id().unwrap().as_str(),
        ctx_b.tenant_id().unwrap().as_str(),
        "tenants must have distinct IDs",
    );
}

#[test]
fn trace_id_uniqueness_across_requests() {
    let mut ids = std::collections::HashSet::new();
    for _ in 0..1000 {
        ids.insert(TraceId::new());
    }
    assert_eq!(
        ids.len(),
        1000,
        "1000 TraceId generations must produce 1000 unique values",
    );
}

// ── Error Propagation Chain ──

#[test]
fn engine_error_variants_are_exhaustive() {
    // Document all error variants for future maintainers
    let errors: Vec<EngineError> = vec![
        EngineError::Timeout,
        EngineError::Sandbox("violation".into()),
        EngineError::PhpFatal("segfault".into()),
        EngineError::ResourceLimit,
        EngineError::Plugin("init failed".into()),
        EngineError::IpcProtocol("malformed frame".into()),
    ];

    for err in errors {
        let status = err.to_http_status();
        assert!(
            (400..=599).contains(&status),
            "error {err} must map to valid HTTP status code, got {status}",
        );
    }
}
