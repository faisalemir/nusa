//! Integration tests for nusa-core task module.
//!
//! Skills applied:
//! - `m07-concurrency`: Async task submission, concurrent execution
//! - `m06-error-handling`: Result propagation, dropped channels
//! - `coding-guidelines`: Descriptive assertions, no unwrap in production paths

use nusa_core::{OffloadTask, TaskManager, TaskResult};
use std::collections::HashMap;
use std::time::Duration;

// ── TaskManager Basic Operations ──

#[test]
fn task_manager_default_creates_valid() {
    let manager = TaskManager::default();
    let (id, _rx) = manager.submit(OffloadTask::Custom {
        task_type: "test".into(),
        payload: serde_json::json!({}),
    });
    assert!(!id.is_empty());
}

#[test]
fn task_manager_new_creates_valid() {
    let manager = TaskManager::new();
    let (id, _rx) = manager.submit(OffloadTask::Custom {
        task_type: "test".into(),
        payload: serde_json::json!({}),
    });
    assert!(!id.is_empty());
}

#[test]
fn task_manager_submit_returns_unique_uuid() {
    let manager = TaskManager::new();
    let mut ids = std::collections::HashSet::new();

    for _ in 0..100 {
        let (id, _rx) = manager.submit(OffloadTask::Custom {
            task_type: "test".into(),
            payload: serde_json::json!({}),
        });
        assert!(ids.insert(id), "Task IDs must be unique");
    }
}

#[test]
fn task_manager_submit_returns_valid_uuid_format() {
    let manager = TaskManager::new();
    let (id, _rx) = manager.submit(OffloadTask::Custom {
        task_type: "test".into(),
        payload: serde_json::json!({}),
    });

    // UUID format: 8-4-4-4-12 (36 chars total)
    assert_eq!(id.len(), 36);
    assert_eq!(id.chars().filter(|c| *c == '-').count(), 4);
    assert!(id.replace('-', "").chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn task_manager_status_returns_not_completed_for_unknown_id() {
    let manager = TaskManager::new();
    let status = manager.status("nonexistent-id");
    assert_eq!(status.task_id, "nonexistent-id");
    assert!(!status.completed);
    assert!(status.result.is_none());
}

#[test]
fn task_manager_status_returns_completed_after_execution() {
    let manager = TaskManager::new();
    let (id, rx) = manager.submit(OffloadTask::Custom {
        task_type: "test".into(),
        payload: serde_json::json!({}),
    });

    // Wait for the async task to complete
    let _ = rx.blocking_recv();
    // Give the spawned task a moment to store the result
    std::thread::sleep(Duration::from_millis(10));

    let status = manager.status(&id);
    assert_eq!(status.task_id, id);
    assert!(status.completed);
    assert!(status.result.is_some());
}

#[test]
fn task_manager_multiple_tasks_independent() {
    let manager = TaskManager::new();

    let (id1, rx1) = manager.submit(OffloadTask::Custom {
        task_type: "task1".into(),
        payload: serde_json::json!({}),
    });
    let (id2, rx2) = manager.submit(OffloadTask::Custom {
        task_type: "task2".into(),
        payload: serde_json::json!({}),
    });

    assert_ne!(id1, id2);

    // Both tasks execute independently via spawned tokio tasks
    let result1 = rx1.blocking_recv();
    let result2 = rx2.blocking_recv();

    assert!(result1.is_ok());
    assert!(result2.is_ok());
}

#[test]
fn task_manager_dropped_receiver_no_panic() {
    let manager = TaskManager::new();
    let (_id, _rx) = manager.submit(OffloadTask::Custom {
        task_type: "test".into(),
        payload: serde_json::json!({}),
    });
    // Receiver dropped immediately — spawned task should handle gracefully
}

// ── OffloadTask Variant Tests ──

#[test]
fn task_http_request_variant() {
    let task = OffloadTask::HttpRequest {
        method: "POST".into(),
        url: "https://api.example.com/data".into(),
        headers: HashMap::from([
            ("Content-Type".into(), "application/json".into()),
        ]),
        body: Some(vec![1, 2, 3]),
    };

    match task {
        OffloadTask::HttpRequest { method, url, headers, body } => {
            assert_eq!(method, "POST");
            assert_eq!(url, "https://api.example.com/data");
            assert_eq!(headers.len(), 1);
            assert!(body.is_some());
        }
        _ => panic!("Expected HttpRequest variant"),
    }
}

#[test]
fn task_http_request_no_body() {
    let task = OffloadTask::HttpRequest {
        method: "GET".into(),
        url: "https://api.example.com".into(),
        headers: HashMap::new(),
        body: None,
    };

    match task {
        OffloadTask::HttpRequest { body, .. } => {
            assert!(body.is_none());
        }
        _ => panic!("Expected HttpRequest variant"),
    }
}

#[test]
fn task_file_operation_variant() {
    let task = OffloadTask::FileOperation {
        operation: "read".into(),
        path: "/data/file.txt".into(),
        data: None,
    };

    match task {
        OffloadTask::FileOperation { operation, path, data } => {
            assert_eq!(operation, "read");
            assert_eq!(path, "/data/file.txt");
            assert!(data.is_none());
        }
        _ => panic!("Expected FileOperation variant"),
    }
}

#[test]
fn task_file_operation_with_data() {
    let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
    let task = OffloadTask::FileOperation {
        operation: "write".into(),
        path: "/data/output.bin".into(),
        data: Some(data.clone()),
    };

    match task {
        OffloadTask::FileOperation { data: Some(d), .. } => {
            assert_eq!(d, data);
        }
        _ => panic!("Expected FileOperation with data"),
    }
}

#[test]
fn task_custom_variant() {
    let payload = serde_json::json!({
        "key": "value",
        "count": 42,
    });
    let task = OffloadTask::Custom {
        task_type: "process_data".into(),
        payload: payload.clone(),
    };

    match task {
        OffloadTask::Custom { task_type, payload: p } => {
            assert_eq!(task_type, "process_data");
            assert_eq!(p, payload);
        }
        _ => panic!("Expected Custom variant"),
    }
}

#[test]
fn task_custom_complex_payload() {
    let payload = serde_json::json!({
        "nested": {
            "array": [1, 2, 3],
            "object": {"key": "value"}
        },
        "string": "hello",
        "number": 3.14,
        "bool": true,
        "null": null
    });

    let task = OffloadTask::Custom {
        task_type: "complex".into(),
        payload: payload.clone(),
    };

    match task {
        OffloadTask::Custom { payload: p, .. } => {
            assert_eq!(p, payload);
        }
        _ => panic!("Expected Custom variant"),
    }
}

// ── TaskResult Tests ──

#[test]
fn task_result_debug() {
    let result = TaskResult {
        success: true,
        data: vec![1, 2, 3],
        error: None,
    };
    let debug = format!("{:?}", result);
    assert!(debug.contains("TaskResult"));
    assert!(debug.contains("success: true"));
}

#[test]
fn task_result_clone() {
    let original = TaskResult {
        success: true,
        data: vec![1, 2, 3],
        error: Some("error".into()),
    };
    let cloned = original.clone();

    assert_eq!(original.success, cloned.success);
    assert_eq!(original.data, cloned.data);
    assert_eq!(original.error, cloned.error);
}

#[test]
fn task_result_serialize_success() {
    let result = TaskResult {
        success: true,
        data: vec![1, 2, 3],
        error: None,
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("true"));
    assert!(json.contains("[1,2,3]"));
}

#[test]
fn task_result_serialize_error() {
    let result = TaskResult {
        success: false,
        data: vec![],
        error: Some("task failed".into()),
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("false"));
    assert!(json.contains("task failed"));
}
