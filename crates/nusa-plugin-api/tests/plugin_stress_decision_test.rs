//! Stress decision tests for plugin error propagation (sector S14).

use std::sync::Arc;
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

struct IntermittentPlugin {
    fail_every: u32,
    calls: Arc<std::sync::atomic::AtomicU32>,
}

#[async_trait::async_trait]
impl Plugin for IntermittentPlugin {
    fn name(&self) -> &'static str {
        "intermittent"
    }

    async fn pre_exec(&self, _ctx: &mut RequestContext) -> nusa_core::Result<()> {
        let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if n.is_multiple_of(self.fail_every) {
            Err(EngineError::Plugin(format!("fail at {n}")))
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn stress_intermittent_pre_failures_propagate() {
    let reg = PluginRegistry::new();
    reg.register(Arc::new(IntermittentPlugin {
        fail_every: 3,
        calls: Arc::new(std::sync::atomic::AtomicU32::new(0)),
    }));

    let mut failures = 0u32;
    for _ in 0..30 {
        let mut ctx = test_context();
        if reg.run_pre_exec(&mut ctx).await.is_err() {
            failures += 1;
        }
    }
    assert!(
        failures >= 5,
        "intermittent failures must surface (got {failures})"
    );
}

struct AlwaysFailPre;

#[async_trait::async_trait]
impl Plugin for AlwaysFailPre {
    fn name(&self) -> &'static str {
        "always-fail"
    }

    async fn pre_exec(&self, _ctx: &mut RequestContext) -> nusa_core::Result<()> {
        Err(EngineError::Plugin("always".into()))
    }
}

#[tokio::test]
async fn stress_always_fail_pre_stable_under_load() {
    let reg = Arc::new(PluginRegistry::new());
    reg.register(Arc::new(AlwaysFailPre));

    let mut handles = Vec::new();
    for _ in 0..50 {
        let reg = reg.clone();
        handles.push(tokio::spawn(async move {
            let mut ctx = test_context();
            reg.run_pre_exec(&mut ctx).await.is_err()
        }));
    }
    let mut err_count = 0;
    for h in handles {
        if h.await.expect("join") {
            err_count += 1;
        }
    }
    assert_eq!(err_count, 50, "every call must fail closed");
}
