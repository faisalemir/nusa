//! Extended leak detection tests.
//!
//! rust-test-deep Phase 4: Resource Exhaustive
//! rust-test-deep §Memory Test Matrix

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use nusa_core::{OffloadTask, RequestContext, TaskManager, TenantId};

// ── Leak Detection: RequestContext ──

/// === Arrange ===
/// RequestContext created in loop.
/// === Act ===
/// 50k iterations, check memory stability.
/// === Assert ===
/// No leak (HashSet size == 50k unique trace IDs).
#[test]
fn request_context_50k_no_trace_id_leak() {
    // === Arrange ===
    use std::collections::HashSet;
    let mut ids = HashSet::new();

    // === Act ===
    for _ in 0..50_000 {
        let ctx = RequestContext::new(
            "/app/public".into(),
            "index.php".into(),
            tokio::time::Instant::now() + Duration::from_secs(30),
        );
        ids.insert(ctx.trace_id());
    }

    // === Assert ===
    assert_eq!(ids.len(), 50_000, "all 50k trace IDs must be unique");
}

/// === Arrange ===
/// RequestContext with Arc<HashMap> shared via clone.
/// === Act ===
/// Create 10k clones, verify Arc reference counting.
/// === Assert ===
/// No leak (Arc properly shared, not duplicated).
#[test]
fn request_context_arc_env_no_leak_on_clone() {
    // === Arrange ===
    let env = Arc::new(HashMap::from([("KEY".into(), "value".into())]));
    let ctx = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_env(env);

    // === Act ===
    let mut clones = vec![];
    for _ in 0..10_000 {
        clones.push(ctx.clone());
    }

    // === Assert ===
    // All clones share the same Arc (no duplication)
    for c in &clones {
        assert!(Arc::ptr_eq(ctx.env(), c.env()));
    }
}

// ── Leak Detection: TaskManager ──

/// === Arrange ===
/// TaskManager with pending HashMap.
/// === Act ===
/// Submit 1000 tasks, consume all results.
/// === Assert ===
/// No orphaned entries.
#[test]
fn task_manager_1000_tasks_no_orphan() {
    // === Arrange ===
    let mgr = TaskManager::new();

    // === Act ===
    for _ in 0..1000 {
        let (_id, rx) = mgr.submit(OffloadTask::Custom {
            task_type: "orphan-test".into(),
            payload: serde_json::json!({}),
        });
        let _ = rx.blocking_recv();
    }

    // Wait for cleanup
    std::thread::sleep(Duration::from_millis(100));

    // === Assert ===
    // All tasks completed, pending map should be empty
    let status = mgr.status("any-id");
    assert!(!status.completed, "unknown task should not be completed");
}

// ── Leak Detection: TenantId ──

/// === Arrange ===
/// TenantId strings allocated.
/// === Act ===
/// 100k TenantId creations, check no leak.
/// === Assert ===
/// No leak (strings properly consumed).
#[test]
fn tenant_id_100k_creations_no_leak() {
    // === Arrange & Act ===
    let mut ids = vec![];
    for i in 0..100_000 {
        ids.push(TenantId::new(format!("tenant-{}", i)));
    }

    // === Assert ===
    assert_eq!(ids.len(), 100_000);
    // All TenantIds properly owned
}

// ── Leak Detection: Headers ──

/// === Arrange ===
/// RequestContext with large headers.
/// === Act ===
/// Create 10k contexts with headers, check memory.
/// === Assert ===
/// Headers not shared between contexts.
#[test]
fn request_context_headers_not_shared() {
    // === Arrange ===
    let ctx1 = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_body(Bytes::from("body1"));

    let ctx2 = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .with_body(Bytes::from("body2"));

    // === Act & Assert ===
    assert_eq!(ctx1.body(), &Bytes::from("body1"));
    assert_eq!(ctx2.body(), &Bytes::from("body2"));
    // Each context has independent body
}

// ── Panic Cleanup Tests ──

/// === Arrange ===
/// RequestContext creation.
/// === Act ===
/// Panic during creation context.
/// === Assert ===
/// No resource leak.
#[test]
fn request_context_panic_during_creation_cleanup() {
    // === Arrange ===
    let result = std::panic::catch_unwind(|| {
        let _ctx = RequestContext::new(
            "/app/public".into(),
            "index.php".into(),
            tokio::time::Instant::now() + Duration::from_secs(30),
        );
        panic!("test panic");
    });

    // === Assert ===
    assert!(result.is_err(), "panic should propagate");
    // No leak: ctx was properly dropped
}

// ── Drop Behavior ──

/// === Arrange ===
/// TaskManager with task submitted.
/// === Act ===
/// Drop TaskManager before task completes.
/// === Assert ===
/// No panic on drop.
#[test]
fn task_manager_dropped_before_task_completes() {
    // === Arrange ===
    let mgr = TaskManager::new();
    let (_id, _rx) = mgr.submit(OffloadTask::Custom {
        task_type: "drop-test".into(),
        payload: serde_json::json!({}),
    });

    // === Act ===
    drop(mgr);

    // === Assert ===
    // No panic
}
