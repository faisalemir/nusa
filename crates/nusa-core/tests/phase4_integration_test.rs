//! Integration tests for Phase 4 features: Multi-tenant isolation & async task offloading.
//!
//! Skills applied:
//! - `m09-domain`: Tenant isolation enforced at gateway level
//! - `m05-type-driven`: TenantId newtype prevents context mixing
//! - `m07-concurrency`: TaskManager thread safety
//! - `m13-domain-error`: Tenant not enabled error handling
//!
//! NOTE: Tenant registry tests are in tenant_test.rs;
//! task variant tests are in task_test.rs.
//! This file contains only unique integration scenarios.

use nusa_core::{OffloadTask, TaskManager};
use std::sync::Arc;
use std::thread;

// ── Task Offloading: Unique Integration Tests ──

/// === Arrange ===
/// TaskManager created, Custom task submitted with empty payload.
/// === Act ===
/// Task submitted, result received via blocking_recv.
/// === Assert ===
/// Result received, task completed, status reflects completion.
#[test]
fn task_manager_submit_custom_returns_result() {
    // === Arrange ===
    let manager = TaskManager::new();

    // === Act ===
    let (id, rx) = manager.submit(OffloadTask::Custom {
        task_type: "integration-test".into(),
        payload: serde_json::json!({}),
    });

    // Wait for the async task to complete
    let result = rx.blocking_recv();

    // === Assert ===
    // Custom tasks return an error result (not implemented)
    assert!(result.is_ok(), "task submission must not fail");
    let result = result.unwrap();
    assert!(!result.success, "custom tasks are not implemented");
    assert!(result.error.is_some(), "custom task must include error message");

    // Verify status shows completed
    let status = manager.status(&id);
    assert!(status.completed, "task status must show completed");
    assert!(status.result.is_some(), "task status must include result");
}

/// === Arrange ===
/// TaskManager wrapped in Arc, 10 threads each submit a task.
/// === Act ===
/// All threads submit concurrently, results collected via join.
/// === Assert ===
/// All 10 results received, all tasks executed independently.
#[test]
fn task_manager_concurrent_submit_no_race() {
    // === Arrange ===
    let manager = Arc::new(TaskManager::new());
    let mut handles = vec![];

    // === Act ===
    for i in 0..10 {
        let mgr = Arc::clone(&manager);
        handles.push(thread::spawn(move || {
            let (_id, rx) = mgr.submit(OffloadTask::Custom {
                task_type: format!("task-{}", i),
                payload: serde_json::json!({ "index": i }),
            });
            rx.blocking_recv()
        }));
    }

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // === Assert ===
    assert_eq!(results.len(), 10, "must have 10 results");

    // All custom tasks return error results (not implemented)
    for r in &results {
        assert!(r.is_ok(), "task submission must not fail");
        assert!(!r.as_ref().unwrap().success, "custom task must not succeed");
    }
}
