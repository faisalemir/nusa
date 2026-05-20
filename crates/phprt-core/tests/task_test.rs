//! Integration tests for phprt-core task module.
//!
//! Skills applied:
//! - `m07-concurrency`: Async task submission, concurrent completion, stress testing
//! - `m06-error-handling`: Result propagation, dropped channels
//! - `coding-guidelines`: Descriptive assertions, no unwrap in production paths

use phprt_core::{OffloadTask, TaskManager, TaskResult};
use std::collections::HashMap;
use std::thread;
use std::time::Duration;

// ── TaskManager Basic Operations ──

#[test]
fn task_manager_default_creates_valid() {
    let manager = TaskManager::default();
    let (id, mut rx) = manager.submit(OffloadTask::Custom {
        task_type: "test".into(),
        payload: serde_json::json!({}),
    });
    assert!(!id.is_empty());
    assert!(rx.try_recv().is_err());
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
    // All non-dash chars should be hex
    assert!(id.replace('-', "").chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn task_manager_complete_sends_result() {
    let manager = TaskManager::new();
    let (id, mut rx) = manager.submit(OffloadTask::Custom {
        task_type: "test".into(),
        payload: serde_json::json!({}),
    });

    manager.complete(&id, TaskResult {
        success: true,
        data: vec![1, 2, 3],
        error: None,
    });

    let result = rx.try_recv();
    assert!(result.is_ok());
    let result = result.unwrap();
    assert!(result.success);
    assert_eq!(result.data, vec![1, 2, 3]);
    assert!(result.error.is_none());
}

#[test]
fn task_manager_complete_with_error() {
    let manager = TaskManager::new();
    let (id, mut rx) = manager.submit(OffloadTask::Custom {
        task_type: "fail".into(),
        payload: serde_json::json!({}),
    });

    manager.complete(&id, TaskResult {
        success: false,
        data: vec![],
        error: Some("Task failed".into()),
    });

    let result = rx.try_recv().unwrap();
    assert!(!result.success);
    assert!(result.error.is_some());
    assert_eq!(result.error.as_deref().unwrap(), "Task failed");
}

#[test]
fn task_manager_complete_unknown_id_no_panic() {
    let manager = TaskManager::new();
    // Complete a task that was never submitted — should not panic
    manager.complete("nonexistent-id", TaskResult {
        success: false,
        data: vec![],
        error: None,
    });
}

#[test]
fn task_manager_multiple_tasks_independent() {
    let manager = TaskManager::new();

    let (id1, mut rx1) = manager.submit(OffloadTask::Custom {
        task_type: "task1".into(),
        payload: serde_json::json!({}),
    });
    let (id2, mut rx2) = manager.submit(OffloadTask::Custom {
        task_type: "task2".into(),
        payload: serde_json::json!({}),
    });

    assert_ne!(id1, id2);

    // Complete task 2 first, then task 1
    manager.complete(&id2, TaskResult {
        success: true,
        data: vec![4, 5, 6],
        error: None,
    });
    manager.complete(&id1, TaskResult {
        success: true,
        data: vec![1, 2, 3],
        error: None,
    });

    assert_eq!(rx1.try_recv().unwrap().data, vec![1, 2, 3]);
    assert_eq!(rx2.try_recv().unwrap().data, vec![4, 5, 6]);
}

#[test]
fn task_manager_complete_dropped_receiver_no_panic() {
    let manager = TaskManager::new();
    let (id, _rx) = manager.submit(OffloadTask::Custom {
        task_type: "test".into(),
        payload: serde_json::json!({}),
    });

    // Receiver is dropped, complete should still succeed
    manager.complete(&id, TaskResult {
        success: true,
        data: vec![1],
        error: None,
    });
}

#[test]
fn task_manager_complete_empty_data() {
    let manager = TaskManager::new();
    let (id, mut rx) = manager.submit(OffloadTask::Custom {
        task_type: "test".into(),
        payload: serde_json::json!({}),
    });

    manager.complete(&id, TaskResult {
        success: true,
        data: vec![],
        error: None,
    });

    let result = rx.try_recv().unwrap();
    assert!(result.data.is_empty());
}

#[test]
fn task_manager_complete_large_data() {
    let manager = TaskManager::new();
    let (id, mut rx) = manager.submit(OffloadTask::Custom {
        task_type: "test".into(),
        payload: serde_json::json!({}),
    });

    let large_data = vec![0xAB; 1024 * 1024]; // 1 MB
    manager.complete(&id, TaskResult {
        success: true,
        data: large_data.clone(),
        error: None,
    });

    let result = rx.try_recv().unwrap();
    assert_eq!(result.data.len(), 1024 * 1024);
    assert_eq!(result.data, large_data);
}

// ── Concurrent Access Tests ──

#[test]
fn task_manager_concurrent_submit_complete() {
    let manager = std::sync::Arc::new(TaskManager::new());
    let mut handles = vec![];

    // Spawn 10 threads that each submit and complete a task
    for i in 0..10 {
        let mgr = std::sync::Arc::clone(&manager);
        handles.push(thread::spawn(move || {
            let (id, mut rx) = mgr.submit(OffloadTask::Custom {
                task_type: format!("task-{}", i),
                payload: serde_json::json!({ "index": i }),
            });

            // Simulate some work
            thread::sleep(Duration::from_millis(10));

            mgr.complete(&id, TaskResult {
                success: true,
                data: vec![i as u8],
                error: None,
            });

            rx.try_recv().unwrap()
        }));
    }

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.len(), 10);

    // All tasks succeeded
    for r in &results {
        assert!(r.success);
    }
}

#[test]
fn task_manager_stress_many_tasks() {
    let manager = TaskManager::new();
    let mut receivers = vec![];

    // Submit 1000 tasks
    for i in 0..1000 {
        let (id, rx) = manager.submit(OffloadTask::Custom {
            task_type: format!("stress-{}", i),
            payload: serde_json::json!({}),
        });
        receivers.push((id, rx));
    }

    // Complete all tasks
    for (id, _) in &receivers {
        manager.complete(id, TaskResult {
            success: true,
            data: vec![0],
            error: None,
        });
    }

    // Verify all receivers got their result
    let mut success_count = 0;
    for (_, mut rx) in receivers {
        if let Ok(result) = rx.try_recv() {
            if result.success {
                success_count += 1;
            }
        }
    }
    assert_eq!(success_count, 1000);
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
