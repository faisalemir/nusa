//! Resource-exhaustive tests for plugin registry (sector S14).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use nusa_core::RequestContext;
use nusa_plugin_api::{Plugin, PluginRegistry};

fn test_context() -> RequestContext {
    RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
}

struct CountPlugin {
    counter: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl Plugin for CountPlugin {
    fn name(&self) -> &'static str {
        "count"
    }

    async fn pre_exec(&self, _ctx: &mut RequestContext) -> nusa_core::Result<()> {
        self.counter.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn resource_register_500_plugins_pre_exec_completes() {
    let counter = Arc::new(AtomicUsize::new(0));
    let reg = PluginRegistry::new();
    for _ in 0..500 {
        reg.register(Arc::new(CountPlugin {
            counter: counter.clone(),
        }));
    }
    let mut ctx = test_context();
    reg.run_pre_exec(&mut ctx).await.expect("500 pre hooks");
    assert_eq!(counter.load(Ordering::SeqCst), 500);
}

#[tokio::test]
async fn resource_repeated_pre_post_cycles_stable() {
    let reg = PluginRegistry::new();
    reg.register(Arc::new(CountPlugin {
        counter: Arc::new(AtomicUsize::new(0)),
    }));

    for _ in 0..200 {
        let mut ctx = test_context();
        reg.run_pre_exec(&mut ctx).await.expect("cycle pre");
        reg.run_post_exec(&ctx).await.expect("cycle post");
    }
}

#[tokio::test]
async fn resource_concurrent_pre_exec_on_shared_registry() {
    let counter = Arc::new(AtomicUsize::new(0));
    let reg = Arc::new(PluginRegistry::new());
    reg.register(Arc::new(CountPlugin {
        counter: counter.clone(),
    }));

    let mut handles = Vec::new();
    for _ in 0..32 {
        let reg = reg.clone();
        handles.push(tokio::spawn(async move {
            let mut ctx = test_context();
            reg.run_pre_exec(&mut ctx).await
        }));
    }
    for h in handles {
        h.await.expect("join").expect("pre");
    }
    assert_eq!(counter.load(Ordering::SeqCst), 32);
}
