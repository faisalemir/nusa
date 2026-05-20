//! Tests for the FFI engine.
//!
//! Note: The FFI engine has a real implementation only on Linux with PHP ZTS headers.
//! On other platforms it returns a stub. These tests verify the public API contract.

use nusa_core::{PhpEngine, RequestContext};
use nusa_engine_ffi::FfiEngine;

#[test]
fn ffi_engine_new() {
    let engine = FfiEngine::new(4);
    let caps = engine.capabilities();
    assert!(caps.contains(&"ffi"));
    assert!(caps.contains(&"zts"));
}

#[test]
fn ffi_engine_capabilities_not_empty() {
    let engine = FfiEngine::new(1);
    assert!(!engine.capabilities().is_empty());
}

#[tokio::test]
async fn ffi_engine_execute_returns_response() {
    let engine = FfiEngine::new(4);
    let ctx = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + std::time::Duration::from_secs(30),
    );
    // On non-Linux this returns the stub; on Linux with ZTS it runs real code.
    let result = engine.execute(ctx).await;
    assert!(result.is_ok(), "execute must always return a response");
    let response = result.unwrap();
    assert!(!response.body.is_empty(), "response body must not be empty");
}

#[tokio::test]
async fn ffi_engine_shutdown_idempotent() {
    let engine = FfiEngine::new(4);
    engine.shutdown().await;
    engine.shutdown().await;
}

#[test]
fn ffi_engine_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<FfiEngine>();
}
