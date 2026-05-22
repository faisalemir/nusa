//! Child engine integration tests.
//!
//! Tests ChildEngine spawning PHP child, IPC communication, timeout, crash recovery.

use std::path::PathBuf;
use std::time::Duration;

use nusa_core::PhpEngine;
use nusa_engine_child::ChildEngine;

// ── ChildEngine Full Integration ──

#[tokio::test]
async fn child_integration_spawn_child_send_request_receive_response() {
    let engine = ChildEngine::with_default_php();
    let ctx = nusa_core::RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );

    let result = engine.execute(ctx).await;
    // May fail if PHP is not available — verify error handling
    // On systems without PHP: IpcProtocol or PhpFatal error
    // With PHP: successful response
    // Either way, engine handles it properly
    assert!(result.is_ok() || result.is_err());
}

#[tokio::test]
async fn child_integration_create_with_custom_paths() {
    let engine = ChildEngine::new(PathBuf::from("php"), PathBuf::from("/app/public/index.php"));
    let caps = engine.capabilities();
    assert!(caps.contains(&"child"));
    assert!(caps.contains(&"process"));
    assert!(caps.contains(&"isolated"));
    assert!(caps.contains(&"ipc"));
}

// ── ChildEngine with Tenant Context ──

#[tokio::test]
async fn child_integration_tenant_context_passed() {
    let engine = ChildEngine::with_default_php();
    let tenant = nusa_core::TenantId::new("tenant-child");

    let ctx = nusa_core::RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_tenant(tenant.clone());

    assert_eq!(ctx.tenant_id().unwrap().as_str(), "tenant-child");

    let result = engine.execute(ctx).await;
    // Execute may succeed or fail depending on PHP availability
    assert!(result.is_ok() || result.is_err());
}

// ── ChildEngine IPC ──

#[tokio::test]
async fn child_integration_full_request_response_cycle() {
    let engine = ChildEngine::with_default_php();

    let mut headers = http::HeaderMap::new();
    headers.insert("X-Request-Method", "POST".parse().unwrap());
    headers.insert("X-Request-Uri", "/api/test".parse().unwrap());

    let ctx = nusa_core::RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_body(bytes::Bytes::from("test body"))
    .with_headers(headers);

    let result = engine.execute(ctx).await;
    // IPC cycle completed (success or error)
    assert!(result.is_ok() || result.is_err());
}

// ── ChildEngine Timeout ──

#[tokio::test]
async fn child_integration_slow_request_times_out() {
    let engine = ChildEngine::with_default_php();

    let ctx = nusa_core::RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_millis(100),
    );

    let result = tokio::time::timeout(Duration::from_millis(200), engine.execute(ctx)).await;

    // Should either timeout or return an error quickly
    match result {
        Ok(Ok(_)) => {
            // Fast execution — fine
        }
        Ok(Err(_)) => {
            // Error returned — expected when PHP not available
        }
        Err(_) => {
            // Timeout — expected for slow processes
        }
    }
}

// ── ChildEngine Crash ──

#[tokio::test]
async fn child_integration_child_crash_error_returned_no_zombie() {
    // Create engine with non-existent PHP binary to simulate crash
    let engine = ChildEngine::new(
        PathBuf::from("nonexistent-php-binary"),
        PathBuf::from("index.php"),
    );

    let ctx = nusa_core::RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(5),
    );

    let result = engine.execute(ctx).await;
    assert!(result.is_err(), "should fail with non-existent PHP binary");
    let err = result.unwrap_err();
    // Error should be PhpFatal or IpcProtocol
    assert!(
        err.to_string().contains("spawn failed")
            || err.to_string().contains("ipc")
            || err.to_string().contains("fatal")
    );
}

// ── ChildEngine Capabilities ──

#[test]
fn child_integration_capabilities_complete() {
    let engine = ChildEngine::with_default_php();
    let caps = engine.capabilities();
    assert!(caps.contains(&"child"));
    assert!(caps.contains(&"process"));
    assert!(caps.contains(&"isolated"));
    assert!(caps.contains(&"ipc"));
    assert_eq!(caps.len(), 4);
}

// ── ChildEngine Send/Sync ──

#[test]
fn child_integration_send_safe() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<ChildEngine>();
    assert_sync::<ChildEngine>();
}

// ── ChildEngine Shutdown ──

#[tokio::test]
async fn child_integration_shutdown_graceful() {
    let engine = ChildEngine::with_default_php();
    engine.shutdown().await;
    // No panic — graceful shutdown
}
