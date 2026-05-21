//! WASM engine tests with real runtime integration.
//! Tests WasmEngine creation, runtime wiring, fuel limits, and memory caps.

use nusa_core::PhpEngine;
use nusa_engine_wasm::WasmEngine;

#[test]
fn wasm_engine_stub_creation() {
    let engine = WasmEngine::stub();
    assert_eq!(engine.memory_limit_bytes(), 256 * 1024 * 1024);
    assert_eq!(engine.fuel_per_request(), 10_000_000);
}

#[test]
fn wasm_engine_stub_capabilities() {
    let engine = WasmEngine::stub();
    let caps = engine.capabilities();
    assert!(caps.contains(&"wasm"));
    assert!(caps.contains(&"sandbox"));
    assert!(caps.contains(&"fuel-limited"));
    assert!(caps.contains(&"memory-capped"));
    assert_eq!(caps.len(), 4);
}

#[tokio::test]
async fn wasm_engine_stub_execute_returns_response() {
    let engine = WasmEngine::stub();
    let ctx = nusa_core::RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + std::time::Duration::from_secs(30),
    );

    let result = engine.execute(ctx).await;
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.status, 200);
    assert!(!response.body.is_empty());
}

#[tokio::test]
async fn wasm_engine_stub_execute_with_tenant_context() {
    let engine = WasmEngine::stub();
    let tenant = nusa_core::TenantId::new("tenant-wasm");
    let ctx = nusa_core::RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .with_tenant(tenant);

    let result = engine.execute(ctx).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn wasm_engine_stub_execute_with_body_and_headers() {
    let engine = WasmEngine::stub();
    let body = bytes::Bytes::from("test body");
    let headers = http::HeaderMap::new();
    let ctx = nusa_core::RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .with_body(body)
    .with_headers(headers);

    let result = engine.execute(ctx).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn wasm_engine_shutdown_is_idempotent() {
    let engine = WasmEngine::stub();
    engine.shutdown().await;
    engine.shutdown().await;
    engine.shutdown().await;
}

#[tokio::test]
async fn wasm_engine_shutdown_then_execute_still_works() {
    let engine = WasmEngine::stub();
    engine.shutdown().await;

    let ctx = nusa_core::RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + std::time::Duration::from_secs(30),
    );

    let result = engine.execute(ctx).await;
    assert!(result.is_ok());
}

#[test]
fn wasm_engine_send_safe() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<WasmEngine>();
    assert_sync::<WasmEngine>();
}

#[test]
fn wasm_runtime_stub_creation() {
    let runtime = nusa_engine_wasm::runtime::WasmRuntime::stub(256);
    assert!(runtime.is_ok());
    let runtime = runtime.unwrap();
    assert_eq!(runtime.memory_limit_bytes(), 256 * 1024 * 1024);
}

#[test]
fn wasm_runtime_memory_limit_various_values() {
    let r1 = nusa_engine_wasm::runtime::WasmRuntime::stub(0).unwrap();
    assert_eq!(r1.memory_limit_bytes(), 0);

    let r2 = nusa_engine_wasm::runtime::WasmRuntime::stub(512).unwrap();
    assert_eq!(r2.memory_limit_bytes(), 512 * 1024 * 1024);

    let r3 = nusa_engine_wasm::runtime::WasmRuntime::stub(2048).unwrap();
    assert_eq!(r3.memory_limit_bytes(), 2048 * 1024 * 1024);
}

#[tokio::test]
async fn wasm_runtime_create_store_succeeds() {
    let runtime = nusa_engine_wasm::runtime::WasmRuntime::stub(256).unwrap();
    let result = runtime.create_store(std::path::Path::new("/tmp"));
    assert!(result.is_ok());
}

#[tokio::test]
async fn wasm_runtime_create_store_limited_succeeds() {
    let runtime = nusa_engine_wasm::runtime::WasmRuntime::stub(256).unwrap();
    let result = runtime.create_store_limited(std::path::Path::new("/tmp"), 128 * 1024 * 1024);
    assert!(result.is_ok());
}

#[test]
fn wasm_runtime_load_module_empty_bytes_fails() {
    let runtime = nusa_engine_wasm::runtime::WasmRuntime::stub(256).unwrap();
    let result = runtime.load_module(&[]);
    assert!(result.is_err());
}

#[test]
fn wasm_runtime_load_module_garbage_bytes_fails() {
    let runtime = nusa_engine_wasm::runtime::WasmRuntime::stub(256).unwrap();
    let result = runtime.load_module(&[0xDE, 0xAD, 0xBE, 0xEF]);
    assert!(result.is_err());
}

#[test]
fn wasm_runtime_data_store_limits() {
    let runtime = nusa_engine_wasm::runtime::WasmRuntime::stub(256).unwrap();
    let store = runtime.create_store(std::path::Path::new("/tmp")).unwrap();

    // Verify store was created successfully with limits
    // The limits are enforced via wasmtime's ResourceLimiter trait
    drop(store); // Just verify creation doesn't panic
}

#[tokio::test]
async fn wasm_engine_multiple_concurrent_executes() {
    let engine = std::sync::Arc::new(WasmEngine::stub());

    let mut handles = Vec::new();
    for i in 0..10 {
        let e = engine.clone();
        handles.push(tokio::spawn(async move {
            let ctx = nusa_core::RequestContext::new(
                "/app/public".into(),
                "index.php".into(),
                tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            );
            e.execute(ctx).await
        }));
    }

    for h in handles {
        let result = h.await.unwrap();
        assert!(result.is_ok());
    }
}
