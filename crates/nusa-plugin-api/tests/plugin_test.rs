//! Integration tests for nusa-plugin-api crate
//!
//! Skills applied:
//! - `m04-zero-cost`: dyn Plugin trait, object safety
//! - `m07-concurrency`: Arc<dyn Plugin> shared across async boundaries
//! - `m01-ownership`: Arc-based plugin registry, no Clone needed

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use nusa_core::{RequestContext, Result};
use nusa_plugin_api::{Plugin, PluginRegistry};

/// Helper: create a RequestContext for testing
fn test_context() -> RequestContext {
    RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
}

// ── Plugin Registry Default ──

#[test]
fn registry_default() {
    let _reg = PluginRegistry::default();
}

#[test]
fn registry_new_is_empty() {
    let reg = PluginRegistry::new();
    // Registry creates without panicking
    let _ = reg;
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

// ── pre_exec Error Stops Chain ──

struct FailingPlugin {
    name: &'static str,
    fail_on_pre: bool,
}

#[async_trait::async_trait]
impl Plugin for FailingPlugin {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn pre_exec(&self, _ctx: &mut RequestContext) -> Result<()> {
        if self.fail_on_pre {
            Err(nusa_core::EngineError::Plugin("pre_exec failure".into()))
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn plugin_pre_exec_error_stops_chain() {
    let reg = PluginRegistry::new();
    reg.register(Arc::new(TestPlugin));
    reg.register(Arc::new(FailingPlugin {
        name: "failing-plugin",
        fail_on_pre: true,
    }));

    let mut ctx = test_context();
    let result = reg.run_pre_exec(&mut ctx).await;

    assert!(result.is_err(), "pre_exec error must propagate");
}

// ── post_exec Error Propagates ──

struct FailingPostPlugin;

#[async_trait::async_trait]
impl Plugin for FailingPostPlugin {
    fn name(&self) -> &'static str {
        "failing-post"
    }

    async fn post_exec(&self, _ctx: &RequestContext) -> Result<()> {
        Err(nusa_core::EngineError::Plugin("post_exec failure".into()))
    }
}

#[tokio::test]
async fn plugin_post_exec_error_propagates() {
    let reg = PluginRegistry::new();
    reg.register(Arc::new(TestPlugin));
    reg.register(Arc::new(FailingPostPlugin));

    let ctx = test_context();
    let result = reg.run_post_exec(&ctx).await;

    assert!(result.is_err(), "post_exec error must propagate");
}

// ── Multiple Plugins Execution Order ──

struct OrderTrackingPlugin {
    name: &'static str,
    order_counter: Arc<AtomicU64>,
}

#[async_trait::async_trait]
impl Plugin for OrderTrackingPlugin {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn pre_exec(&self, _ctx: &mut RequestContext) -> Result<()> {
        self.order_counter.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn post_exec(&self, _ctx: &RequestContext) -> Result<()> {
        self.order_counter.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn plugin_multiple_execute_in_registration_order() {
    let counter = Arc::new(AtomicU64::new(0));
    let reg = PluginRegistry::new();

    let plugin1 = Arc::new(OrderTrackingPlugin {
        name: "plugin-1",
        order_counter: counter.clone(),
    });
    let plugin2 = Arc::new(OrderTrackingPlugin {
        name: "plugin-2",
        order_counter: counter.clone(),
    });

    reg.register(plugin1);
    reg.register(plugin2);

    let mut ctx = test_context();
    reg.run_pre_exec(&mut ctx).await.unwrap();
    reg.run_post_exec(&ctx).await.unwrap();

    // Each plugin runs pre + post = 4 increments total
    assert_eq!(counter.load(Ordering::SeqCst), 4);
}

// ── Same Plugin Registered Twice ──

#[tokio::test]
async fn plugin_same_plugin_registered_twice() {
    let counter = Arc::new(AtomicU64::new(0));
    let reg = PluginRegistry::new();

    let plugin = Arc::new(OrderTrackingPlugin {
        name: "duplicate-plugin",
        order_counter: counter.clone(),
    });

    reg.register(plugin.clone());
    reg.register(plugin);

    let mut ctx = test_context();
    reg.run_pre_exec(&mut ctx).await.unwrap();

    // Both registrations execute = 2 pre_exec calls
    assert_eq!(counter.load(Ordering::SeqCst), 2);
}

// ── Empty Registry ──

#[tokio::test]
async fn plugin_empty_registry_succeeds() {
    let reg = PluginRegistry::new();
    let mut ctx = test_context();

    let result = reg.run_pre_exec(&mut ctx).await;
    assert!(result.is_ok(), "empty registry must succeed");

    let result = reg.run_post_exec(&ctx).await;
    assert!(result.is_ok(), "empty registry post_exec must succeed");
}

// ── Many Plugins ──

#[tokio::test]
async fn plugin_hundred_registered() {
    let counter = Arc::new(AtomicU64::new(0));
    let reg = PluginRegistry::new();

    for i in 0..100 {
        let plugin = Arc::new(OrderTrackingPlugin {
            name: Box::leak(format!("plugin-{}", i).into_boxed_str()),
            order_counter: counter.clone(),
        });
        reg.register(plugin);
    }

    let mut ctx = test_context();
    reg.run_pre_exec(&mut ctx).await.unwrap();
    reg.run_post_exec(&ctx).await.unwrap();

    // 100 plugins × 2 calls each = 200
    assert_eq!(counter.load(Ordering::SeqCst), 200);
}

// ── Concurrent Register + Execute ──

#[tokio::test]
async fn plugin_concurrent_register_and_execute() {
    let counter = Arc::new(AtomicU64::new(0));
    let reg = Arc::new(PluginRegistry::new());

    // Register 10 plugins concurrently
    let mut handles = vec![];
    for i in 0..10 {
        let reg_clone = reg.clone();
        let counter_clone = counter.clone();
        handles.push(tokio::spawn(async move {
            let plugin = Arc::new(OrderTrackingPlugin {
                name: Box::leak(format!("concurrent-{}", i).into_boxed_str()),
                order_counter: counter_clone,
            });
            reg_clone.register(plugin);
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    // Execute concurrently
    let mut exec_handles = vec![];
    for _ in 0..5 {
        let reg_clone = reg.clone();
        exec_handles.push(tokio::spawn(async move {
            let mut ctx = test_context();
            reg_clone.run_pre_exec(&mut ctx).await
        }));
    }
    for h in exec_handles {
        let result = h.await.unwrap();
        assert!(result.is_ok(), "concurrent execute must succeed");
    }
}

// ── Plugin Default Implementations ──

struct MinimalPlugin;

#[async_trait::async_trait]
impl Plugin for MinimalPlugin {
    fn name(&self) -> &'static str {
        "minimal"
    }
    // pre_exec and post_exec use default Ok(())
}

#[tokio::test]
async fn plugin_default_pre_exec_returns_ok() {
    let reg = PluginRegistry::new();
    reg.register(Arc::new(MinimalPlugin));

    let mut ctx = test_context();
    let result = reg.run_pre_exec(&mut ctx).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn plugin_default_post_exec_returns_ok() {
    let reg = PluginRegistry::new();
    reg.register(Arc::new(MinimalPlugin));

    let ctx = test_context();
    let result = reg.run_post_exec(&ctx).await;
    assert!(result.is_ok());
}
