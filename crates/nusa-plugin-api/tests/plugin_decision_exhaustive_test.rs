//! Decision-exhaustive tests for plugin hook chains (sector S14).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use nusa_core::{EngineError, RequestContext};
use nusa_plugin_api::{Plugin, PluginRegistry};

fn test_context() -> RequestContext {
    RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
}

struct MarkPlugin {
    name: &'static str,
    marker: Arc<AtomicUsize>,
    phase: u8,
}

#[async_trait::async_trait]
impl Plugin for MarkPlugin {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn pre_exec(&self, _ctx: &mut RequestContext) -> nusa_core::Result<()> {
        if self.phase == 0 {
            self.marker.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    }

    async fn post_exec(&self, _ctx: &RequestContext) -> nusa_core::Result<()> {
        if self.phase == 1 {
            self.marker.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    }
}

struct FailPrePlugin;

#[async_trait::async_trait]
impl Plugin for FailPrePlugin {
    fn name(&self) -> &'static str {
        "fail-pre"
    }

    async fn pre_exec(&self, _ctx: &mut RequestContext) -> nusa_core::Result<()> {
        Err(EngineError::Plugin("pre".into()))
    }
}

struct FailPostPlugin;

#[async_trait::async_trait]
impl Plugin for FailPostPlugin {
    fn name(&self) -> &'static str {
        "fail-post"
    }

    async fn post_exec(&self, _ctx: &RequestContext) -> nusa_core::Result<()> {
        Err(EngineError::Plugin("post".into()))
    }
}

#[tokio::test]
async fn decision_pre_fail_stops_later_pre_hooks() {
    let marker = Arc::new(AtomicUsize::new(0));
    let reg = PluginRegistry::new();
    reg.register(Arc::new(FailPrePlugin));
    reg.register(Arc::new(MarkPlugin {
        name: "after-fail",
        marker: marker.clone(),
        phase: 0,
    }));

    let mut ctx = test_context();
    assert!(reg.run_pre_exec(&mut ctx).await.is_err());
    assert_eq!(
        marker.load(Ordering::SeqCst),
        0,
        "plugins after failing pre_exec must not run"
    );
}

#[tokio::test]
async fn decision_post_fail_stops_later_post_hooks() {
    let marker = Arc::new(AtomicUsize::new(0));
    let reg = PluginRegistry::new();
    reg.register(Arc::new(MarkPlugin {
        name: "before-fail",
        marker: marker.clone(),
        phase: 1,
    }));
    reg.register(Arc::new(FailPostPlugin));
    reg.register(Arc::new(MarkPlugin {
        name: "after-fail",
        marker: marker.clone(),
        phase: 1,
    }));

    let ctx = test_context();
    assert!(reg.run_post_exec(&ctx).await.is_err());
    assert_eq!(
        marker.load(Ordering::SeqCst),
        1,
        "only hooks before failing post_exec may run"
    );
}

#[tokio::test]
async fn decision_empty_registry_pre_and_post_ok() {
    let reg = PluginRegistry::new();
    let mut ctx = test_context();
    reg.run_pre_exec(&mut ctx).await.expect("empty pre");
    reg.run_post_exec(&ctx).await.expect("empty post");
}

#[tokio::test]
async fn decision_all_ok_runs_pre_then_post_for_each() {
    let marker = Arc::new(AtomicUsize::new(0));
    let reg = PluginRegistry::new();
    for i in 0..5 {
        reg.register(Arc::new(MarkPlugin {
            name: Box::leak(format!("p{i}").into_boxed_str()),
            marker: marker.clone(),
            phase: 0,
        }));
    }
    for i in 0..5 {
        reg.register(Arc::new(MarkPlugin {
            name: Box::leak(format!("p{i}-post").into_boxed_str()),
            marker: marker.clone(),
            phase: 1,
        }));
    }
    let mut ctx = test_context();
    reg.run_pre_exec(&mut ctx).await.expect("pre");
    reg.run_post_exec(&ctx).await.expect("post");
    assert_eq!(marker.load(Ordering::SeqCst), 10);
}
