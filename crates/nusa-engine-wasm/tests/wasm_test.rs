//! Detailed integration tests for nusa-engine-wasm — Phase 3 WASM sandbox engine.
//!
//! Skills applied:
//! - `m03-mutability`: StoreLimits interior mutability
//! - `m06-error-handling`: WASM trap → EngineError::Sandbox
//! - `m12-lifecycle`: init → execute → shutdown phases

use nusa_core::{PhpEngine, RequestContext, TenantId};
use nusa_engine_wasm::WasmEngine;
use nusa_engine_wasm::runtime::WasmRuntime;
use std::path::PathBuf;
use std::time::Duration;

// ---------------------------------------------------------------------------
// WasmEngine stub construction
// ---------------------------------------------------------------------------

#[test]
fn wasm_engine_stub_creates_successfully() {
    let engine = WasmEngine::stub();
    drop(engine);
}

#[test]
fn wasm_engine_stub_memory_limit_256mb() {
    let engine = WasmEngine::stub();
    assert_eq!(engine.memory_limit_bytes(), 256 * 1024 * 1024);
}

#[test]
fn wasm_engine_stub_fuel_limit_10m() {
    let engine = WasmEngine::stub();
    assert_eq!(engine.fuel_per_request(), 10_000_000);
}

// ---------------------------------------------------------------------------
// WasmEngine execute (async)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn wasm_engine_execute_returns_ok() {
    let engine = WasmEngine::stub();
    let ctx = RequestContext::new(
        PathBuf::from("/app/public"),
        PathBuf::from("index.php"),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );
    let result = engine.execute(ctx).await;
    assert!(result.is_ok(), "execute must return Ok for stub engine");
}

#[tokio::test]
async fn wasm_engine_execute_returns_200_status() {
    let engine = WasmEngine::stub();
    let ctx = RequestContext::new(
        PathBuf::from("/app/public"),
        PathBuf::from("index.php"),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );
    let resp = engine.execute(ctx).await.unwrap();
    assert_eq!(resp.status, 200);
}

#[tokio::test]
async fn wasm_engine_execute_returns_non_empty_body() {
    let engine = WasmEngine::stub();
    let ctx = RequestContext::new(
        PathBuf::from("/app/public"),
        PathBuf::from("index.php"),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );
    let resp = engine.execute(ctx).await.unwrap();
    assert!(!resp.body.is_empty(), "body must not be empty");
}

#[tokio::test]
async fn wasm_engine_execute_returns_empty_headers() {
    let engine = WasmEngine::stub();
    let ctx = RequestContext::new(
        PathBuf::from("/app/public"),
        PathBuf::from("index.php"),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );
    let resp = engine.execute(ctx).await.unwrap();
    assert!(resp.headers.is_empty(), "stub must return no headers");
}

#[tokio::test]
async fn wasm_engine_execute_with_body_and_headers() {
    let engine = WasmEngine::stub();
    let mut headers = http::HeaderMap::new();
    headers.insert("x-test", "value".parse().unwrap());
    let body = bytes::Bytes::from("test body");
    let ctx = RequestContext::new(
        PathBuf::from("/app/public"),
        PathBuf::from("index.php"),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_headers(headers)
    .with_body(body);

    let resp = engine.execute(ctx).await.unwrap();
    assert_eq!(resp.status, 200);
}

#[tokio::test]
async fn wasm_engine_execute_with_tenant_context() {
    let engine = WasmEngine::stub();
    let ctx = RequestContext::new(
        PathBuf::from("/app/public"),
        PathBuf::from("index.php"),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_tenant(TenantId::new("acme-corp"));

    let result = engine.execute(ctx).await;
    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// WasmEngine capabilities
// ---------------------------------------------------------------------------

#[test]
fn wasm_engine_capabilities_contains_wasm() {
    let engine = WasmEngine::stub();
    assert!(engine.capabilities().contains(&"wasm"));
}

#[test]
fn wasm_engine_capabilities_contains_sandbox() {
    let engine = WasmEngine::stub();
    assert!(engine.capabilities().contains(&"sandbox"));
}

#[test]
fn wasm_engine_capabilities_contains_fuel_limited() {
    let engine = WasmEngine::stub();
    assert!(engine.capabilities().contains(&"fuel-limited"));
}

#[test]
fn wasm_engine_capabilities_contains_memory_capped() {
    let engine = WasmEngine::stub();
    assert!(engine.capabilities().contains(&"memory-capped"));
}

#[test]
fn wasm_engine_capabilities_has_expected_count() {
    let engine = WasmEngine::stub();
    assert_eq!(engine.capabilities().len(), 4);
}

// ---------------------------------------------------------------------------
// WasmEngine shutdown
// ---------------------------------------------------------------------------

#[tokio::test]
async fn wasm_engine_shutdown_does_not_panic() {
    let engine = WasmEngine::stub();
    engine.shutdown().await;
}

#[tokio::test]
async fn wasm_engine_shutdown_is_idempotent() {
    let engine = WasmEngine::stub();
    engine.shutdown().await;
    engine.shutdown().await;
}

#[tokio::test]
async fn wasm_engine_shutdown_then_execute() {
    let engine = WasmEngine::stub();
    engine.shutdown().await;

    let ctx = RequestContext::new(
        PathBuf::from("/app/public"),
        PathBuf::from("index.php"),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );
    let result = engine.execute(ctx).await;
    assert!(result.is_ok(), "stub engine still executes after shutdown");
}

// ---------------------------------------------------------------------------
// WasmEngine Send/Sync safety
// ---------------------------------------------------------------------------

#[tokio::test]
async fn wasm_engine_execute_is_send_safe() {
    let engine = std::sync::Arc::new(WasmEngine::stub());
    let ctx = RequestContext::new(
        PathBuf::from("/app/public"),
        PathBuf::from("index.php"),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );

    let handle = tokio::spawn({
        let engine = engine.clone();
        async move { engine.execute(ctx).await }
    });

    let result = handle.await.unwrap();
    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// WasmRuntime
// ---------------------------------------------------------------------------

#[tokio::test]
async fn wasm_runtime_creates_with_valid_memory_limit() {
    let runtime = WasmRuntime::new(&[], 512).expect("runtime must be created");
    assert_eq!(runtime.memory_limit_bytes(), 512 * 1024 * 1024);
}

#[tokio::test]
async fn wasm_runtime_memory_limit_zero_bytes() {
    let runtime = WasmRuntime::new(&[], 0).expect("runtime must be created");
    assert_eq!(runtime.memory_limit_bytes(), 0);
}

#[tokio::test]
async fn wasm_runtime_memory_limit_large() {
    let runtime = WasmRuntime::new(&[], 2048).expect("runtime must be created");
    assert_eq!(runtime.memory_limit_bytes(), 2048 * 1024 * 1024);
}

#[test]
fn wasm_runtime_create_store_succeeds() {
    let runtime = WasmRuntime::new(&[], 256).expect("runtime must be created");
    let store = runtime.create_store(&PathBuf::from("/tmp/test"));
    assert!(store.is_ok(), "create_store must succeed on stub");
}

#[test]
fn wasm_runtime_create_store_with_nonexistent_dir() {
    let runtime = WasmRuntime::new(&[], 256).expect("runtime must be created");
    let store = runtime.create_store(&PathBuf::from("/nonexistent/dir"));
    assert!(store.is_ok(), "create_store must succeed even with nonexistent dir");
}

#[test]
fn wasm_runtime_load_module_empty_bytes_fails() {
    let runtime = WasmRuntime::new(&[], 256).expect("runtime must be created");
    let result = runtime.load_module(&[]);
    assert!(result.is_err(), "empty bytes must fail module loading");
}

#[test]
fn wasm_runtime_load_module_garbage_bytes_fails() {
    let runtime = WasmRuntime::new(&[], 256).expect("runtime must be created");
    let garbage = vec![0xDE, 0xAD, 0xBE, 0xEF];
    let result = runtime.load_module(&garbage);
    assert!(result.is_err(), "garbage bytes must fail module loading");
}
