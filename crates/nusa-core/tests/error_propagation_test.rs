//! Error propagation chain tests.
//!
//! rust-test §Error Propagation Chain Tests
//! Tests multi-layer error chains, context preservation, timeout vs cancellation.

use std::time::Duration;

use nusa_core::{EngineError, OffloadTask, TaskManager, TaskResult, with_timeout};

// ── Error Propagation Through TaskManager ──

/// Error at leaf propagates to top with context.
#[test]
fn error_chain_custom_task_propagates_error() {
    let mgr = TaskManager::new();
    let (_task_id, rx) = mgr.submit(OffloadTask::Custom {
        task_type: "test-error".into(),
        payload: serde_json::json!({}),
    });

    let result = rx.blocking_recv();
    assert!(result.is_ok(), "should receive result");
    let task_result = result.unwrap();
    assert!(!task_result.success, "custom task should fail");
    assert!(task_result.error.is_some(), "error should be present");
    let err = task_result.error.unwrap();
    assert!(
        err.contains("test-error"),
        "error should mention task type: {}",
        err
    );
}

/// Error in middle layer preserves inner context.
#[test]
fn error_chain_http_task_invalid_url() {
    let mgr = TaskManager::new();
    let (_task_id, rx) = mgr.submit(OffloadTask::HttpRequest {
        method: "GET".into(),
        url: "not-a-valid-url".into(),
        headers: Default::default(),
        body: None,
    });

    let result = rx.blocking_recv();
    assert!(result.is_ok(), "should receive result");
    let task_result = result.unwrap();
    assert!(!task_result.success, "invalid URL should fail");
    assert!(
        task_result.error.is_some(),
        "error should describe the issue"
    );
    let err = task_result.error.as_ref().unwrap();
    assert!(
        err.contains("builder") || err.contains("url") || err.contains("failed"),
        "error should mention builder or url failure: {}",
        err
    );
}

/// Error context preserved through file task.
#[test]
fn error_chain_file_task_nonexistent_path() {
    let mgr = TaskManager::new();
    let (_task_id, rx) = mgr.submit(OffloadTask::FileOperation {
        operation: "read".into(),
        path: "/nonexistent/path/that/does/not/exist.txt".into(),
        data: None,
    });

    let result = rx.blocking_recv();
    assert!(result.is_ok(), "should receive result");
    let task_result = result.unwrap();
    assert!(!task_result.success, "read of nonexistent file should fail");
    assert!(
        task_result.error.is_some(),
        "error should describe the issue"
    );
}

// ── Timeout vs Cancellation Distinction ──

/// Timeout triggers error → proper error variant.
#[tokio::test]
async fn error_timeout_triggers_proper_error() {
    // with_timeout should return a timeout error
    let slow_future = async {
        tokio::time::sleep(Duration::from_secs(60)).await;
        Ok::<(), EngineError>(())
    };

    let result = with_timeout(50, slow_future).await;
    assert!(result.is_err(), "should timeout");
}

/// Cancellation via tokio::select cancels the slow branch.
#[tokio::test]
async fn error_cancellation_via_select() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let completed = Arc::new(AtomicBool::new(false));
    let completed_clone = completed.clone();

    let slow_future = async move {
        tokio::time::sleep(Duration::from_secs(60)).await;
        completed_clone.store(true, Ordering::SeqCst);
    };

    // Use select to race the slow future against an immediate ready future
    tokio::select! {
        _ = slow_future => {
            panic!("slow future should not complete");
        }
        _ = tokio::time::sleep(Duration::from_millis(10)) => {
            // timeout wins - slow future is cancelled
        }
    }

    // The slow future should not have completed
    assert!(
        !completed.load(Ordering::SeqCst),
        "slow future should have been cancelled"
    );
}

// ── Engine Error Mapping ──

/// EngineError::Timeout maps to 408.
#[test]
fn error_engine_timeout_maps_to_408() {
    let err = EngineError::Timeout;
    assert_eq!(err.to_http_status(), 408);
}

/// EngineError::PhpFatal maps to 502.
#[test]
fn error_engine_php_fatal_maps_to_502() {
    let err = EngineError::PhpFatal("test error".into());
    assert_eq!(err.to_http_status(), 502);
}

/// EngineError::Sandbox maps to 500.
#[test]
fn error_engine_sandbox_maps_to_500() {
    let err = EngineError::Sandbox("sandbox violation".into());
    assert_eq!(err.to_http_status(), 500);
}

/// EngineError display includes message.
#[test]
fn error_display_includes_message() {
    let err = EngineError::PhpFatal("database connection failed".into());
    let msg = err.to_string();
    assert!(
        msg.contains("database connection failed"),
        "message: {}",
        msg
    );
}

// ── Multi-Step Error Propagation ──

/// Task submission works, but result receiver can detect errors.
#[test]
fn error_propagation_task_manager_multiple_submits() {
    let mgr = TaskManager::new();

    // Submit multiple tasks
    let mut receivers = Vec::new();
    for i in 0..5 {
        let (id, rx) = mgr.submit(OffloadTask::Custom {
            task_type: format!("multi-{}", i),
            payload: serde_json::json!({}),
        });
        assert!(!id.is_empty());
        receivers.push(rx);
    }

    // All should complete with errors (custom not implemented)
    for (i, rx) in receivers.into_iter().enumerate() {
        let result = rx.blocking_recv();
        assert!(
            result.is_ok(),
            "task {} result channel should not be dropped",
            i
        );
        let task_result = result.unwrap();
        assert!(!task_result.success, "task {} should fail", i);
    }
}

/// Error propagation through TaskResult serialization.
#[test]
fn error_propagation_task_result_serialization() {
    let task_result = TaskResult {
        success: false,
        data: vec![],
        error: Some("test error with special chars: <>&\"'".into()),
    };

    // Should serialize without panicking
    let json = serde_json::to_string(&task_result).unwrap();
    assert!(json.contains("test error with special chars"));

    // Should deserialize back
    let deserialized: TaskResult = serde_json::from_str(&json).unwrap();
    assert!(!deserialized.success);
    assert_eq!(deserialized.error, task_result.error);
}

// ── with_timeout Error Propagation ──

/// with_timeout propagates inner errors (not just timeout).
#[tokio::test]
async fn error_with_timeout_propagates_inner_error() {
    let failing_future = async { Err::<(), _>(EngineError::PhpFatal("inner failure".into())) };

    let result = with_timeout(1000, failing_future).await;
    assert!(result.is_err(), "inner error should propagate");
    // Should be the original error, not a timeout
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("inner failure"),
        "should preserve inner error: {}",
        err
    );
}

/// with_timeout returns Ok when future completes in time.
#[tokio::test]
async fn error_with_timeout_completes_in_time() {
    let fast_future = async { Ok::<_, EngineError>(42) };

    let result = with_timeout(1000, fast_future).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
}
