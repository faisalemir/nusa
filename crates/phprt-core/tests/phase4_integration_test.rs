//! Integration tests for Phase 4 features: Multi-tenant isolation & async task offloading.
//!
//! Skills applied:
//! - `m09-domain`: Tenant isolation enforced at gateway level
//! - `m05-type-driven`: TenantId newtype prevents context mixing
//! - `m07-concurrency`: TaskManager thread safety
//! - `m13-domain-error`: Tenant not enabled error handling

use phprt_core::{
    OffloadTask, TaskManager, TaskResult,
    TenantConfig, TenantId, TenantRegistry,
};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

// ── Multi-Tenant Gateway Tests ──

#[test]
fn tenant_registry_register_and_query() {
    let mut registry = TenantRegistry::new();

    let config = TenantConfig {
        id: TenantId::new("acme-corp"),
        vfs_root: "/tenants/acme".into(),
        max_memory_mb: 512,
        max_requests_per_minute: 1000,
        enabled: true,
    };
    registry.register(config);

    let retrieved = registry.get(&TenantId::new("acme-corp"));
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().max_memory_mb, 512);
}

#[test]
fn tenant_registry_enabled_check() {
    let mut registry = TenantRegistry::new();

    registry.register(TenantConfig {
        id: TenantId::new("active"),
        vfs_root: "/tenants/active".into(),
        max_memory_mb: 256,
        max_requests_per_minute: 100,
        enabled: true,
    });
    registry.register(TenantConfig {
        id: TenantId::new("disabled"),
        vfs_root: "/tenants/disabled".into(),
        max_memory_mb: 256,
        max_requests_per_minute: 100,
        enabled: false,
    });

    assert!(registry.is_enabled(&TenantId::new("active")));
    assert!(!registry.is_enabled(&TenantId::new("disabled")));
    assert!(!registry.is_enabled(&TenantId::new("unknown")));
}

#[test]
fn tenant_registry_multiple_tenants_isolation() {
    let mut registry = TenantRegistry::new();

    for i in 0..10 {
        registry.register(TenantConfig {
            id: TenantId::new(&format!("tenant-{}", i)),
            vfs_root: format!("/tenants/tenant-{}", i),
            max_memory_mb: 256 * (i + 1),
            max_requests_per_minute: 1000,
            enabled: i % 2 == 0,
        });
    }

    // Verify isolation: each tenant has independent config
    for i in 0..10 {
        let id = TenantId::new(&format!("tenant-{}", i));
        let config = registry.get(&id).unwrap();
        assert_eq!(config.max_memory_mb, 256 * (i + 1));
        assert_eq!(config.enabled, i % 2 == 0);
    }
}

#[test]
fn tenant_id_uniqueness() {
    let a = TenantId::new("unique-1");
    let b = TenantId::new("unique-2");
    assert_ne!(a, b);
}

#[test]
fn tenant_id_display_format() {
    let id = TenantId::new("my-tenant");
    assert_eq!(format!("{}", id), "my-tenant");
}

#[test]
fn tenant_config_serialization() {
    let config = TenantConfig {
        id: TenantId::new("acme"),
        vfs_root: "/tenants/acme".into(),
        max_memory_mb: 512,
        max_requests_per_minute: 1000,
        enabled: true,
    };

    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("acme"));
    assert!(json.contains("512"));

    let deserialized: TenantConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.max_memory_mb, 512);
}

// ── Task Offloading Tests ──

#[test]
fn task_manager_submit_returns_uuid() {
    let manager = TaskManager::new();
    let (id, _rx) = manager.submit(OffloadTask::Custom {
        task_type: "test".into(),
        payload: serde_json::json!({}),
    });

    // UUID format: 8-4-4-4-12 (36 chars)
    assert_eq!(id.len(), 36);
    assert_eq!(id.chars().filter(|c| *c == '-').count(), 4);
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

    let result = rx.try_recv().unwrap();
    assert!(result.success);
    assert_eq!(result.data, vec![1, 2, 3]);
}

#[test]
fn task_manager_complete_unknown_id_no_panic() {
    let manager = TaskManager::new();
    manager.complete("nonexistent-id", TaskResult {
        success: false,
        data: vec![],
        error: None,
    });
}

#[test]
fn task_manager_concurrent_submit_complete() {
    let manager = Arc::new(TaskManager::new());
    let mut handles = vec![];

    for i in 0..10 {
        let mgr = Arc::clone(&manager);
        handles.push(thread::spawn(move || {
            let (id, mut rx) = mgr.submit(OffloadTask::Custom {
                task_type: format!("task-{}", i),
                payload: serde_json::json!({ "index": i }),
            });

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

    for r in &results {
        assert!(r.success);
    }
}

#[test]
fn task_http_request_variant() {
    let task = OffloadTask::HttpRequest {
        method: "POST".into(),
        url: "https://api.example.com/data".into(),
        headers: std::collections::HashMap::from([
            ("Content-Type".into(), "application/json".into()),
        ]),
        body: Some(vec![1, 2, 3]),
    };

    match task {
        OffloadTask::HttpRequest { method, url, .. } => {
            assert_eq!(method, "POST");
            assert_eq!(url, "https://api.example.com/data");
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
        OffloadTask::FileOperation { operation, path, .. } => {
            assert_eq!(operation, "read");
            assert_eq!(path, "/data/file.txt");
        }
        _ => panic!("Expected FileOperation variant"),
    }
}

#[test]
fn task_result_serialization() {
    let result = TaskResult {
        success: true,
        data: vec![1, 2, 3],
        error: Some("error".into()),
    };

    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("true"));
    assert!(json.contains("[1,2,3]"));
    assert!(json.contains("error"));

    let deserialized: TaskResult = serde_json::from_str(&json).unwrap();
    assert!(deserialized.success);
    assert_eq!(deserialized.data, vec![1, 2, 3]);
}
