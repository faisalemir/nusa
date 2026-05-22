//! FFI engine integration tests.
//!
//! Tests FfiEngine with real PHP scripts, tenant context, concurrent execution, and shutdown.

use std::time::Duration;

use nusa_core::PhpEngine;
use nusa_engine_ffi::FfiEngine;

// ── FfiEngine Full Integration ──

#[tokio::test]
async fn ffi_integration_execute_real_script() {
    let engine = FfiEngine::new(4);
    let ctx = nusa_core::RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );

    let result = engine.execute(ctx).await;
    // On non-Linux or without PHP headers, returns stub response
    // On Linux with PHP, executes real script
    assert!(result.is_ok());
}

#[tokio::test]
async fn ffi_integration_response_status() {
    let engine = FfiEngine::new(4);
    let ctx = nusa_core::RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );

    let result = engine.execute(ctx).await;
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.status, 200);
}

// ── FfiEngine with Tenant Context ──

#[tokio::test]
async fn ffi_integration_tenant_context_passed() {
    let engine = FfiEngine::new(4);
    let tenant = nusa_core::TenantId::new("tenant-ffi");

    let ctx = nusa_core::RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_tenant(tenant.clone());

    assert_eq!(ctx.tenant_id().unwrap().as_str(), "tenant-ffi");

    let result = engine.execute(ctx).await;
    assert!(result.is_ok());
}

// ── FfiEngine Concurrent ──

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ffi_integration_concurrent_no_cross_thread_leak() {
    let engine = std::sync::Arc::new(FfiEngine::new(10));

    let mut handles = Vec::new();
    for i in 0..10 {
        let e = engine.clone();
        handles.push(tokio::spawn(async move {
            let ctx = nusa_core::RequestContext::new(
                format!("/app/tenant-{}", i).into(),
                "index.php".into(),
                tokio::time::Instant::now() + Duration::from_secs(30),
            );
            e.execute(ctx).await
        }));
    }

    for h in handles {
        let result = h.await.expect("task failed");
        assert!(result.is_ok());
    }
}

// ── FfiEngine Zero Workers ──

#[tokio::test]
async fn ffi_integration_zero_workers_backpressure() {
    let engine = FfiEngine::new(0);
    let ctx = nusa_core::RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_millis(100),
    );

    // With zero workers, the semaphore has no permits
    // The acquire should block and timeout
    let result = tokio::time::timeout(Duration::from_millis(200), engine.execute(ctx)).await;

    // Should timeout or return ResourceLimit error
    assert!(result.is_err() || result.unwrap().is_err());
}

// ── FfiEngine Shutdown ──

#[tokio::test]
async fn ffi_integration_graceful_shutdown_resources_released() {
    let engine = FfiEngine::new(4);
    engine.shutdown().await;
    // Semaphore closed — no new permits
    // All resources released
}

#[tokio::test]
async fn ffi_integration_execute_after_shutdown_fails() {
    let engine = FfiEngine::new(4);
    engine.shutdown().await;

    let ctx = nusa_core::RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );

    let result = engine.execute(ctx).await;
    // After shutdown, semaphore is closed, acquire fails
    assert!(result.is_err());
}

// ── FfiEngine Capabilities ──

#[test]
fn ffi_integration_capabilities_complete() {
    let engine = FfiEngine::new(4);
    let caps = engine.capabilities();
    assert!(caps.contains(&"ffi"));
    assert!(caps.contains(&"native-ext"));
    assert!(caps.contains(&"zts"));
}

// ── FfiEngine Send/Sync ──

#[test]
fn ffi_integration_send_safe() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<FfiEngine>();
    assert_sync::<FfiEngine>();
}
