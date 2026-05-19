//! Integration tests for phprt-plugin-api crate
//!
//! Skills applied:
//! - `m04-zero-cost`: dyn Plugin trait, object safety
//! - `m07-concurrency`: Arc<dyn Plugin> shared across async boundaries
//! - `m01-ownership`: Arc-based plugin registry, no Clone needed

use std::sync::Arc;
use std::time::Duration;

use phprt_core::RequestContext;
use phprt_plugin_api::{Plugin, PluginRegistry};

/// Helper: create a RequestContext for testing
fn test_context() -> RequestContext {
    RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
}

// ── Plugin Registry Lifecycle ──

#[test]
fn registry_default() {
    let _reg = PluginRegistry::default();
}

// ── Plugin Trait Object ──

struct TestPlugin;

#[async_trait::async_trait]
impl Plugin for TestPlugin {
    fn name(&self) -> &'static str {
        "test-plugin"
    }
}

#[tokio::test]
async fn plugin_registry_accepts_and_runs_plugin() {
    let reg = PluginRegistry::new();
    reg.register(Arc::new(TestPlugin));

    let mut ctx = test_context();
    reg.run_pre_exec(&mut ctx).await.unwrap();
    reg.run_post_exec(&ctx).await.unwrap();
}
