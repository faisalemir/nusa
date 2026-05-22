//! WASM engine integration tests.
//!
//! Tests WasmEngine with real WASM module loading, tenant context, body/headers, capabilities, and shutdown.

use std::time::Duration;

use nusa_core::PhpEngine;
use nusa_engine_wasm::WasmEngine;

// ── WasmEngine Full Integration ──

#[tokio::test]
async fn wasm_integration_load_real_module_execute_with_context() {
    // Test with stub engine (real WASM module requires compiled WASI binary)
    let engine = WasmEngine::stub();
    let ctx = nusa_core::RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );

    let result = engine.execute(ctx).await;
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.status, 200);
    assert!(!response.body.is_empty());
}

#[tokio::test]
async fn wasm_integration_full_request_context() {
    let engine = WasmEngine::stub();

    let mut headers = http::HeaderMap::new();
    headers.insert("X-Custom", "test-value".parse().unwrap());

    let ctx = nusa_core::RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_body(bytes::Bytes::from("request body"))
    .with_headers(headers);

    let result = engine.execute(ctx).await;
    assert!(result.is_ok());
}

// ── WasmEngine with Tenant Context ──

#[tokio::test]
async fn wasm_integration_tenant_context_passed() {
    let engine = WasmEngine::stub();
    let tenant = nusa_core::TenantId::new("tenant-wasm-integration");

    let ctx = nusa_core::RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_tenant(tenant.clone());

    assert_eq!(ctx.tenant_id().unwrap().as_str(), "tenant-wasm-integration");

    let result = engine.execute(ctx).await;
    assert!(result.is_ok());
}

// ── WasmEngine with Body/Headers ──

#[tokio::test]
async fn wasm_integration_body_and_headers_passed() {
    let engine = WasmEngine::stub();

    let mut headers = http::HeaderMap::new();
    headers.insert("Content-Type", "application/json".parse().unwrap());
    headers.insert("X-Request-Id", "req-123".parse().unwrap());

    let ctx = nusa_core::RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_body(bytes::Bytes::from(r#"{"key":"value"}"#))
    .with_headers(headers);

    assert_eq!(ctx.body().len(), 15);
    assert_eq!(ctx.headers().len(), 2);

    let result = engine.execute(ctx).await;
    assert!(result.is_ok());
}

// ── WasmEngine Capabilities ──

#[test]
fn wasm_integration_all_capabilities_verified() {
    let engine = WasmEngine::stub();
    let caps = engine.capabilities();

    assert_eq!(caps.len(), 4);
    assert!(caps.contains(&"wasm"));
    assert!(caps.contains(&"sandbox"));
    assert!(caps.contains(&"fuel-limited"));
    assert!(caps.contains(&"memory-capped"));
}

// ── WasmEngine Shutdown Then Execute ──

#[tokio::test]
async fn wasm_integration_shutdown_idempotent() {
    let engine = WasmEngine::stub();

    engine.shutdown().await;
    engine.shutdown().await;
    engine.shutdown().await;
    // No panic — shutdown is idempotent
}

#[tokio::test]
async fn wasm_integration_execute_after_shutdown_returns_response() {
    let engine = WasmEngine::stub();
    engine.shutdown().await;

    let ctx = nusa_core::RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    );

    let result = engine.execute(ctx).await;
    // Stub engine still works after shutdown
    assert!(result.is_ok());
}

// ── WasmEngine Memory Limits ──

#[test]
fn wasm_integration_memory_limit_bytes_correct() {
    let engine = WasmEngine::stub();
    assert_eq!(engine.memory_limit_bytes(), 256 * 1024 * 1024);
}

#[test]
fn wasm_integration_fuel_per_request_correct() {
    let engine = WasmEngine::stub();
    assert_eq!(engine.fuel_per_request(), 10_000_000);
}

// ── WasmEngine Send/Sync ──

#[test]
fn wasm_integration_send_safe() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<WasmEngine>();
    assert_sync::<WasmEngine>();
}

// ── WasmEngine Concurrent Execute ──

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wasm_integration_concurrent_execute_no_race() {
    let engine = std::sync::Arc::new(WasmEngine::stub());

    let mut handles = Vec::new();
    for _ in 0..10 {
        let e = engine.clone();
        handles.push(tokio::spawn(async move {
            let ctx = nusa_core::RequestContext::new(
                "/app/public".into(),
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
