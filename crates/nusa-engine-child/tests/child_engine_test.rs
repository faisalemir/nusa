//! Tests for the child process engine stub.
//!
//! Note: The child engine is currently a stub (M1/M2 milestone).
//! These tests verify stub behavior and public API contracts.

use nusa_core::{PhpEngine, RequestContext};
use nusa_engine_child::ChildEngine;
use std::path::PathBuf;

#[test]
fn child_engine_with_default_php() {
    let engine = ChildEngine::with_default_php();
    let caps = engine.capabilities();
    assert!(caps.contains(&"child"));
    assert!(caps.contains(&"process"));
}

#[test]
fn child_engine_new() {
    let engine = ChildEngine::new(PathBuf::from("/usr/bin/php"));
    assert_eq!(engine.capabilities().len(), 3);
}

#[test]
fn child_engine_capabilities_not_empty() {
    let engine = ChildEngine::with_default_php();
    assert!(!engine.capabilities().is_empty());
}

#[tokio::test]
async fn child_engine_execute_stub() {
    let engine = ChildEngine::with_default_php();
    let ctx = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + std::time::Duration::from_secs(30),
    );
    let result = engine.execute(ctx).await;
    assert!(result.is_ok(), "stub execute must return Ok");
    let response = result.unwrap();
    assert_eq!(response.status, 200);
    assert!(!response.body.is_empty(), "stub response must have body");
}

#[tokio::test]
async fn child_engine_shutdown_idempotent() {
    let engine = ChildEngine::with_default_php();
    // Stub shutdown should not panic even without spawned processes
    engine.shutdown().await;
    engine.shutdown().await; // Second call must also be safe
}

#[test]
fn child_engine_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ChildEngine>();
}
