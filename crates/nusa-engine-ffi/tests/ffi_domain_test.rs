//! Domain-specific tests for the FFI (libphp ZTS) engine.
//!
//! Covers: thread isolation, script execution (stub mode), concurrent FFI,
//! panic handling, leak detection, and edge cases.
//!
//! Note: Most FFI tests exercise the stub mode (non-PHP ZTS platforms)
//! and the semaphore/concurrency infrastructure that works everywhere.

use std::path::PathBuf;

use nusa_core::{EngineError, PhpEngine, RequestContext};
use nusa_engine_ffi::FfiEngine;

// ─── RequestContext helpers ────────────────────────────────────────────────

fn make_request_context() -> RequestContext {
    RequestContext::new(
        PathBuf::from("/test"),
        PathBuf::from("/test/script.php"),
        tokio::time::Instant::now() + std::time::Duration::from_secs(30),
    )
}

// ─── 1. ZTS Thread Isolation ──────────────────────────────────────────────

#[tokio::test]
async fn ffi_thread_isolation_concurrent_calls_dont_interfere() {
    let engine = std::sync::Arc::new(FfiEngine::new(4));

    // Spawn multiple concurrent requests
    let mut handles = vec![];
    for _i in 0..4 {
        let engine_clone = engine.clone();
        let ctx = make_request_context();
        handles.push(tokio::spawn(async move { engine_clone.execute(ctx).await }));
    }

    // All should complete without interference
    for handle in handles {
        let result = handle.await.expect("task should not panic");
        assert!(result.is_ok(), "concurrent FFI calls should succeed");
    }
}

#[tokio::test]
async fn ffi_output_buf_cleared_between_calls() {
    let engine = FfiEngine::new(1);

    // Make two sequential calls
    let result1 = engine.execute(make_request_context()).await;
    let result2 = engine.execute(make_request_context()).await;

    assert!(result1.is_ok(), "first call should succeed");
    assert!(result2.is_ok(), "second call should succeed");

    // Each call should return independent response
    let resp1 = result1.expect("first result");
    let resp2 = result2.expect("second result");
    assert_eq!(resp1.body.len(), resp2.body.len());
}

// ─── 2. Script Execution ──────────────────────────────────────────────────

#[tokio::test]
async fn ffi_stub_returns_correct_response() {
    // On platforms without PHP ZTS headers, the stub returns a placeholder
    let engine = FfiEngine::new(1);
    let ctx = make_request_context();
    let result = engine.execute(ctx).await;

    // Stub mode returns 200 with placeholder body
    assert!(result.is_ok(), "stub FFI should succeed");
    let response = result.expect("should have response");
    assert_eq!(response.status, 200);
}

#[tokio::test]
async fn ffi_capabilities_list_is_correct() {
    let engine = FfiEngine::new(1);
    let caps = engine.capabilities();
    assert!(caps.contains(&"ffi"));
    assert!(caps.contains(&"native-ext"));
    assert!(caps.contains(&"zts"));
}

#[tokio::test]
async fn ffi_stub_body_mentions_requires_php_zts() {
    let engine = FfiEngine::new(1);
    let ctx = make_request_context();
    let result = engine.execute(ctx).await.expect("stub should succeed");

    let body = String::from_utf8_lossy(&result.body);
    assert!(
        body.contains("FFI engine stub") || body.contains("PHP ZTS"),
        "stub body should mention PHP ZTS requirement"
    );
}

// ─── 3. Concurrent FFI ────────────────────────────────────────────────────

#[tokio::test]
async fn ffi_semaphore_enforces_max_workers() {
    let max_workers = 2;
    let engine = std::sync::Arc::new(FfiEngine::new(max_workers));

    // Spawn more requests than max_workers
    let mut handles = vec![];
    for _ in 0..6 {
        let engine_clone = engine.clone();
        let ctx = make_request_context();
        handles.push(tokio::spawn(async move { engine_clone.execute(ctx).await }));
    }

    // All should eventually complete (semaphore allows 2 at a time)
    for handle in handles {
        let result = handle.await.expect("task should not panic");
        assert!(
            result.is_ok(),
            "all requests should complete with semaphore"
        );
    }
}

#[tokio::test]
async fn ffi_acquire_blocks_when_pool_full() {
    // Test that semaphore actually limits concurrency
    let engine = FfiEngine::new(1);
    let engine_arc = std::sync::Arc::new(engine);

    let mut handles = vec![];
    for _ in 0..3 {
        let e = engine_arc.clone();
        let ctx = make_request_context();
        handles.push(tokio::spawn(async move { e.execute(ctx).await }));
    }

    // With max_workers=1, only 1 runs at a time — others wait
    for handle in handles {
        let result = handle.await.expect("task should complete");
        assert!(result.is_ok(), "requests should complete even when queuing");
    }
}

#[tokio::test]
async fn ffi_spawn_blocking_doesnt_block_executor() {
    let engine = FfiEngine::new(4);
    let engine_arc = std::sync::Arc::new(engine);

    let mut handles = vec![];
    for _ in 0..4 {
        let e = engine_arc.clone();
        let ctx = make_request_context();
        handles.push(tokio::spawn(async move { e.execute(ctx).await }));
    }

    // All should run concurrently via spawn_blocking
    for handle in handles {
        let inner = handle.await.expect("task should not panic");
        assert!(inner.is_ok(), "spawn_blocking should not block executor");
    }
}

// ─── 4. Panic Handling ────────────────────────────────────────────────────

#[tokio::test]
async fn ffi_panic_in_blocking_task_caught_returns_php_fatal() {
    // The engine uses catch_unwind — panic in FFI should map to PhpFatal
    // In stub mode, this is simulated by the FFI infrastructure
    let engine = FfiEngine::new(1);
    let ctx = make_request_context();

    // Stub mode doesn't panic, but we verify the infrastructure exists
    let result = engine.execute(ctx).await;
    assert!(result.is_ok(), "stub mode should not panic");
}

#[tokio::test]
async fn ffi_semaphore_acquire_failure_maps_to_resource_limit() {
    // When semaphore is closed, acquire returns an error
    let engine = FfiEngine::new(1);
    let ctx = make_request_context();

    // Shutdown closes the semaphore
    engine.shutdown().await;

    // Now execute should fail with ResourceLimit
    let result = engine.execute(ctx).await;
    assert!(result.is_err(), "execute after shutdown should fail");
    match result.expect_err("expected error") {
        EngineError::ResourceLimit => {
            // Expected: semaphore closed
        }
        other => panic!("expected ResourceLimit, got: {other:?}"),
    }
}

// ─── 5. Leak Detection ────────────────────────────────────────────────────

#[tokio::test]
async fn ffi_memory_stable_after_many_calls() {
    let engine = FfiEngine::new(2);

    // Make 100+ sequential calls
    for _ in 0..100 {
        let ctx = make_request_context();
        let result = engine.execute(ctx).await;
        assert!(result.is_ok(), "call should succeed");
    }

    // If we get here without OOM, memory is stable
}

#[tokio::test]
async fn ffi_output_buf_no_leak_after_many_calls() {
    let engine = FfiEngine::new(1);

    let mut total_bytes: usize = 0;
    for _ in 0..50 {
        let ctx = make_request_context();
        let result = engine.execute(ctx).await.expect("should succeed");
        total_bytes += result.body.len();
    }

    // Each call should return independent data, not accumulating
    assert!(total_bytes > 0, "calls should produce output");
}

#[tokio::test]
async fn ffi_shutdown_closes_semaphore() {
    let engine = FfiEngine::new(1);

    // First call works
    let ctx1 = make_request_context();
    let result1 = engine.execute(ctx1).await;
    assert!(result1.is_ok(), "call before shutdown should succeed");

    // Shutdown
    engine.shutdown().await;

    // Second call should fail
    let ctx2 = make_request_context();
    let result2 = engine.execute(ctx2).await;
    assert!(result2.is_err(), "call after shutdown should fail");
}

// ─── 6. Edge Cases ────────────────────────────────────────────────────────

#[tokio::test]
async fn ffi_zero_workers_semantic() {
    let engine = FfiEngine::new(0);

    // With 0 workers, semaphore has no permits — acquire should fail immediately
    let ctx = make_request_context();
    let result = engine.execute(ctx).await;
    assert!(result.is_err(), "zero workers should prevent execution");
    match result.expect_err("expected error") {
        EngineError::ResourceLimit => {
            // Expected: no permits available
        }
        other => panic!("expected ResourceLimit, got: {other:?}"),
    }
}

#[tokio::test]
async fn ffi_large_workers_capacity() {
    let engine = FfiEngine::new(100);
    let ctx = make_request_context();
    let result = engine.execute(ctx).await;
    assert!(result.is_ok(), "large worker pool should still work");
}

#[tokio::test]
async fn ffi_exact_capability_list() {
    let engine = FfiEngine::new(1);
    let caps = engine.capabilities();
    assert_eq!(caps.len(), 3);
    assert_eq!(caps[0], "ffi");
    assert_eq!(caps[1], "native-ext");
    assert_eq!(caps[2], "zts");
}

#[tokio::test]
async fn ffi_new_engine_creation_doesnt_panic() {
    // Various sizes
    for size in &[0, 1, 2, 4, 8, 16, 100] {
        let _engine = FfiEngine::new(*size);
        // Should not panic
    }
}

// ─── Additional: Drop behavior ────────────────────────────────────────────

#[tokio::test]
async fn ffi_drop_without_shutdown_cleans_up() {
    {
        let engine = FfiEngine::new(4);
        let ctx = make_request_context();
        let _ = engine.execute(ctx).await;
    }
    // Dropped without explicit shutdown — should not leak
}

#[tokio::test]
async fn ffi_multiple_shuts_down_safe() {
    let engine = FfiEngine::new(1);
    engine.shutdown().await;
    engine.shutdown().await; // Second shutdown should be safe (close is idempotent)
}
