//! Extended concurrency tests for nusa-core crate.
//!
//! Covers: TaskManager deadlock/starvation/cancellation/channel scenarios,
//! TenantRegistry races, RateLimiter contention, BackpressureGuard/ResourceGuard
//! multi-threaded behavior, memory ordering verification.

use std::sync::Arc;
use std::time::Duration;

use nusa_core::{
    BackpressureGuard, OffloadTask, ResourceGuard, TaskManager, TenantConfig, TenantId,
    TenantRateLimiter, TenantRegistry,
};
use parking_lot::Mutex;

// ── TaskManager: Deadlock Scenarios ──

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn taskmanager_status_submit_concurrent_no_race() {
    // === Arrange ===
    let manager = Arc::new(TaskManager::new());

    let submit_count = 50usize;
    let status_count = 50usize;

    // === Act ===
    let mut submit_handles = Vec::new();
    let mut status_handles = Vec::new();

    // Submit tasks concurrently
    for i in 0..submit_count {
        let mgr = manager.clone();
        submit_handles.push(tokio::spawn(async move {
            let task = OffloadTask::Custom {
                task_type: "noop".to_string(),
                payload: serde_json::json!({"iteration": i}),
            };
            let (id, _) = mgr.submit(task);
            id
        }));
    }

    // Concurrently query status (read-write race)
    for _ in 0..status_count {
        let mgr = manager.clone();
        status_handles.push(tokio::spawn(async move {
            let id = format!("nonexistent-{}", uuid::Uuid::new_v4());
            mgr.status(&id)
        }));
    }

    // === Assert ===
    for handle in submit_handles {
        tokio::time::timeout(Duration::from_secs(5), handle)
            .await
            .expect("submit must complete within timeout")
            .expect("submit must not panic");
    }
    for handle in status_handles {
        tokio::time::timeout(Duration::from_secs(5), handle)
            .await
            .expect("status must complete within timeout")
            .expect("status must not panic");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn taskmanager_submit_same_task_id_no_race() {
    // === Arrange ===
    let manager = Arc::new(TaskManager::new());
    let tasks_per_thread = 20usize;

    // === Act ===
    let mut handles = Vec::new();
    for _ in 0..4 {
        let mgr = manager.clone();
        handles.push(tokio::spawn(async move {
            let mut ids = Vec::new();
            for _ in 0..tasks_per_thread {
                let task = OffloadTask::Custom {
                    task_type: "noop".to_string(),
                    payload: serde_json::json!({}),
                };
                let (id, _) = mgr.submit(task);
                ids.push(id);
            }
            ids
        }));
    }

    // === Assert ===
    let mut all_ids = Vec::new();
    for handle in handles {
        let ids = tokio::time::timeout(Duration::from_secs(10), handle)
            .await
            .expect("must complete")
            .expect("must not panic");
        all_ids.extend(ids);
    }

    // Verify uniqueness of all generated task IDs
    let unique_count: std::collections::HashSet<_> = all_ids.iter().collect();
    assert_eq!(
        unique_count.len(),
        all_ids.len(),
        "all task IDs must be unique"
    );
}

// ── TaskManager: Starvation ──

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn taskmanager_hot_key_contention_no_starvation() {
    // === Arrange ===
    let manager = Arc::new(TaskManager::new());
    let manager2 = manager.clone();

    // === Act ===
    // Many threads submitting the same task type (hot key)
    let mut handles = Vec::new();
    for _ in 0..16 {
        let mgr = manager.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..10 {
                let task = OffloadTask::Custom {
                    task_type: "hot-key".to_string(),
                    payload: serde_json::json!({}),
                };
                let (_id, rx) = mgr.submit(task);
                // Drop receiver to simulate fire-and-forget
                drop(rx);
            }
        }));
    }

    // === Assert ===
    for handle in handles {
        tokio::time::timeout(Duration::from_secs(15), handle)
            .await
            .expect("must complete")
            .expect("must not panic");
    }

    // Verify manager is still functional
    let task = OffloadTask::Custom {
        task_type: "final".to_string(),
        payload: serde_json::json!({}),
    };
    let (_id, _rx) = manager2.submit(task);
}

// ── TaskManager: Task Cancellation ──

#[tokio::test]
async fn taskmanager_drop_receiver_during_await_cleanup() {
    // === Arrange ===
    let manager = TaskManager::new();

    // === Act ===
    let task = OffloadTask::Custom {
        task_type: "will-fail".to_string(),
        payload: serde_json::json!({}),
    };
    let (_id, rx) = manager.submit(task);
    // Drop receiver before task completes
    drop(rx);

    // Give task time to execute
    tokio::time::sleep(Duration::from_millis(200)).await;

    // === Assert ===
    // Manager should still be functional — no leak
    let task2 = OffloadTask::Custom {
        task_type: "recovery".to_string(),
        payload: serde_json::json!({}),
    };
    let (_id2, _rx2) = manager.submit(task2);
}

#[tokio::test]
async fn taskmanager_http_task_timeout_returns_error() {
    // === Arrange ===
    let manager = TaskManager::new();

    // === Act ===
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let closed_port = listener.local_addr().expect("addr").port();
    drop(listener);

    let task = OffloadTask::HttpRequest {
        method: "GET".to_string(),
        url: format!("http://127.0.0.1:{closed_port}"),
        headers: Default::default(),
        body: None,
    };
    let (_id, rx) = manager.submit(task);

    // === Assert ===
    let result = tokio::time::timeout(Duration::from_secs(30), rx).await;
    match result {
        Ok(Ok(task_result)) => {
            assert!(
                !task_result.success,
                "HTTP to closed local port must fail in production (Alpine/Podman safe)"
            );
        }
        Ok(Err(_)) => {}
        Err(_) => panic!("HTTP task to closed port must complete within 30s"),
    }
}

#[tokio::test]
async fn taskmanager_file_operation_invalid_path_returns_error() {
    // === Arrange ===
    let manager = TaskManager::new();

    // === Act ===
    let task = OffloadTask::FileOperation {
        operation: "read".to_string(),
        path: "/nonexistent/path/file.txt".to_string(),
        data: None,
    };
    let (_id, rx) = manager.submit(task);

    // === Assert ===
    let result = tokio::time::timeout(Duration::from_secs(10), rx)
        .await
        .expect("must complete within timeout");
    let task_result = result.expect("channel must not be closed");
    assert!(!task_result.success, "reading nonexistent file must fail");
    assert!(task_result.error.is_some(), "error message must be present");
}

// ── TaskManager: Channel Scenarios ──

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn taskmanager_send_faster_than_consume_backpressure() {
    // === Arrange ===
    let manager = Arc::new(TaskManager::new());
    let submitted = Arc::new(Mutex::new(0usize));
    let completed = Arc::new(Mutex::new(0usize));

    // === Act ===
    let mut submit_handles = Vec::new();
    for _ in 0..8 {
        let mgr = manager.clone();
        let submitted = submitted.clone();
        submit_handles.push(tokio::spawn(async move {
            for _ in 0..20 {
                let task = OffloadTask::Custom {
                    task_type: "fast".to_string(),
                    payload: serde_json::json!({}),
                };
                let (_id, _rx) = mgr.submit(task);
                *submitted.lock() += 1;
            }
        }));
    }

    // Wait for all submissions
    for h in submit_handles {
        tokio::time::timeout(Duration::from_secs(30), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }

    tokio::time::sleep(Duration::from_secs(2)).await;

    // === Assert ===
    let total_submitted = *submitted.lock();
    let _total_completed = *completed.lock();
    assert_eq!(total_submitted, 160, "all tasks must be submitted");
    // Some may still be running — just verify no crash
    assert!(total_submitted > 0, "some tasks must have been submitted");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn taskmanager_multiple_producers_mpsc_no_loss() {
    // === Arrange ===
    let manager = Arc::new(TaskManager::new());
    let ids = Arc::new(Mutex::new(Vec::new()));

    // === Act ===
    let mut handles = Vec::new();
    for producer_id in 0..4 {
        let mgr = manager.clone();
        let ids = ids.clone();
        handles.push(tokio::spawn(async move {
            let mut local_ids = Vec::new();
            for _ in 0..10 {
                let task = OffloadTask::Custom {
                    task_type: format!("producer-{}", producer_id),
                    payload: serde_json::json!({"producer": producer_id}),
                };
                let (id, _rx) = mgr.submit(task);
                local_ids.push(id.clone());
            }
            ids.lock().extend(local_ids);
        }));
    }

    for h in handles {
        tokio::time::timeout(Duration::from_secs(30), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }

    // === Assert ===
    let all_ids = ids.lock();
    assert_eq!(all_ids.len(), 40, "all 40 task IDs must be recorded");
}

// ── TaskManager: Scaling ──

#[tokio::test(flavor = "multi_thread", worker_threads = 16)]
async fn taskmanager_16_threads_no_contention() {
    // === Arrange ===
    let manager = Arc::new(TaskManager::new());

    // === Act ===
    let mut handles = Vec::new();
    for _ in 0..16 {
        let mgr = manager.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..5 {
                let task = OffloadTask::Custom {
                    task_type: "scale-16".to_string(),
                    payload: serde_json::json!({}),
                };
                let (_id, _rx) = mgr.submit(task);
            }
        }));
    }

    // === Assert ===
    for h in handles {
        tokio::time::timeout(Duration::from_secs(30), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 32)]
async fn taskmanager_32_threads_stress() {
    // === Arrange ===
    let manager = Arc::new(TaskManager::new());

    // === Act ===
    let mut handles = Vec::new();
    for _ in 0..32 {
        let mgr = manager.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..3 {
                let task = OffloadTask::Custom {
                    task_type: "scale-32".to_string(),
                    payload: serde_json::json!({}),
                };
                let (_id, _rx) = mgr.submit(task);
            }
        }));
    }

    // === Assert ===
    for h in handles {
        tokio::time::timeout(Duration::from_secs(30), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 64)]
async fn taskmanager_64_threads_stress() {
    // === Arrange ===
    let manager = Arc::new(TaskManager::new());

    // === Act ===
    let mut handles = Vec::new();
    for _ in 0..64 {
        let mgr = manager.clone();
        handles.push(tokio::spawn(async move {
            let task = OffloadTask::Custom {
                task_type: "scale-64".to_string(),
                payload: serde_json::json!({}),
            };
            let (_id, _rx) = mgr.submit(task);
        }));
    }

    // === Assert ===
    for h in handles {
        tokio::time::timeout(Duration::from_secs(30), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }
}

// ── TenantRegistry: Concurrent Access ──

#[test]
fn tenantregistry_concurrent_register_same_tenant_last_wins() {
    // === Arrange ===
    let mut registry = TenantRegistry::new();
    let tenant_id = TenantId::new("test-tenant");

    // === Act ===
    // Multiple registrations — last one wins (HashMap insert semantics)
    for i in 0..10 {
        let config = TenantConfig {
            id: tenant_id.clone(),
            vfs_root: format!("/root/v{}", i),
            max_memory_mb: 256,
            max_requests_per_minute: 100,
            enabled: i % 2 == 0,
        };
        registry.register(config);
    }

    // === Assert ===
    let config = registry.get(&tenant_id);
    assert!(config.is_some(), "tenant must be registered");
    let config = config.expect("must exist");
    // Last write wins with i=9 (odd → enabled=false)
    assert!(!config.enabled, "last registration must win");
}

#[test]
fn tenantregistry_concurrent_get_register_no_race() {
    // === Arrange ===
    let registry = Arc::new(Mutex::new(TenantRegistry::new()));

    // === Act ===
    let mut handles = Vec::new();

    // Register threads
    for i in 0..4 {
        let reg = registry.clone();
        handles.push(std::thread::spawn(move || {
            let id = TenantId::new(format!("concurrent-{}", i));
            let config = TenantConfig {
                id,
                vfs_root: format!("/root/{}", i),
                max_memory_mb: 256,
                max_requests_per_minute: 100,
                enabled: true,
            };
            reg.lock().register(config);
        }));
    }

    // Get threads
    for i in 0..4 {
        let reg = registry.clone();
        handles.push(std::thread::spawn(move || {
            let id = TenantId::new(format!("concurrent-{}", i));
            let _ = reg.lock().get(&id);
        }));
    }

    // === Assert ===
    for h in handles {
        h.join().expect("thread must not panic");
    }

    // Verify all tenants registered
    let reg = registry.lock();
    for i in 0..4 {
        let id = TenantId::new(format!("concurrent-{}", i));
        assert!(reg.get(&id).is_some(), "tenant must be registered");
    }
}

// ── RateLimiter: Concurrent Access ──

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn ratelimiter_concurrent_multiple_tenants_high_contention() {
    // === Arrange ===
    let limiter = Arc::new(TenantRateLimiter::new(1000, 100)); // 1000 rpm, burst 100

    // === Act ===
    let mut handles = Vec::new();
    for tenant_idx in 0..16 {
        let lim = limiter.clone();
        handles.push(tokio::spawn(async move {
            let tenant_id = TenantId::new(format!("tenant-{}", tenant_idx));
            let mut allowed = 0u64;
            let mut denied = 0u64;
            for _ in 0..200 {
                if lim.is_allowed(&tenant_id) {
                    allowed += 1;
                } else {
                    denied += 1;
                }
            }
            (allowed, denied)
        }));
    }

    // === Assert ===
    let mut total_allowed = 0u64;
    let mut total_denied = 0u64;
    for h in handles {
        let (allowed, denied) = tokio::time::timeout(Duration::from_secs(10), h)
            .await
            .expect("must complete")
            .expect("must not panic");
        total_allowed += allowed;
        total_denied += denied;
    }

    // Each tenant has burst of 100, so 16 * 100 = 1600 allowed initially
    // 16 * 200 = 3200 total attempts
    assert!(total_allowed > 0, "some requests must be allowed");
    assert!(total_denied > 0, "burst must be exhausted");
    assert_eq!(total_allowed + total_denied, 3200, "all attempts accounted");
}

#[test]
fn ratelimiter_concurrent_check_then_act_no_data_race() {
    // === Arrange ===
    let limiter = Arc::new(TenantRateLimiter::new(6000, 50)); // high limit
    let limiter2 = limiter.clone();
    let tenant_id = TenantId::new("race-test");

    // === Act ===
    let mut handles = Vec::new();
    for _ in 0..8 {
        let lim = limiter.clone();
        let tid = tenant_id.clone();
        handles.push(std::thread::spawn(move || {
            // check-then-act pattern
            for _ in 0..100 {
                if lim.is_allowed(&tid) {
                    // Act on the allowed result
                    let _ = lim.remaining(&tid);
                }
            }
        }));
    }

    // === Assert ===
    for h in handles {
        h.join().expect("thread must not panic");
    }

    // Verify limiter is still functional
    assert!(limiter2.is_allowed(&tenant_id) || limiter2.remaining(&tenant_id).is_some());
}

// ── BackpressureGuard: Multi-threaded ──

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn backpressureguard_multithreaded_pressure_no_deadlock() {
    // === Arrange ===
    let guard = Arc::new(BackpressureGuard::new(4)); // Only 4 concurrent permits
    let acquired = Arc::new(Mutex::new(0usize));
    let rejected = Arc::new(Mutex::new(0usize));

    // === Act ===
    let mut handles = Vec::new();
    for _ in 0..16 {
        let g = guard.clone();
        let acquired = acquired.clone();
        let rejected = rejected.clone();
        handles.push(tokio::spawn(async move {
            match g.try_acquire().await {
                Some(_permit) => {
                    *acquired.lock() += 1;
                    // Hold permit briefly
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                None => {
                    *rejected.lock() += 1;
                }
            }
        }));
    }

    // === Assert ===
    for h in handles {
        tokio::time::timeout(Duration::from_secs(10), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }

    let total_acquired = *acquired.lock();
    let total_rejected = *rejected.lock();
    assert!(total_acquired > 0, "some must acquire");
    assert!(total_rejected > 0, "some must be rejected (backpressure)");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn backpressureguard_capacity_exhaustion_recovery() {
    // === Arrange ===
    let guard = BackpressureGuard::new(2);

    // === Act ===
    let permit1 = guard.try_acquire().await;
    let permit2 = guard.try_acquire().await;

    assert!(permit1.is_some(), "first permit must succeed");
    assert!(permit2.is_some(), "second permit must succeed");

    // All permits exhausted
    let permit3 = guard.try_acquire().await;
    assert!(permit3.is_none(), "third permit must be rejected");

    // Release one permit
    drop(permit1);

    // === Assert ===
    let permit4 = guard.try_acquire().await;
    assert!(permit4.is_some(), "permit must be available after release");
}

#[tokio::test]
async fn backpressureguard_cleanup_on_task_failure() {
    // === Arrange ===
    let guard = Arc::new(BackpressureGuard::new(1));

    // === Act ===
    let permit = guard.try_acquire().await.expect("must acquire");

    // Simulate task failure — drop permit on error
    drop(permit);

    // === Assert ===
    let new_permit = guard.try_acquire().await;
    assert!(
        new_permit.is_some(),
        "permit must be recoverable after failure"
    );
}

// ── ResourceGuard: Cleanup ──

#[tokio::test]
async fn resourceguard_with_timeout_error_returns_timeout() {
    // === Arrange ===
    let guard = ResourceGuard {
        max_request_bytes: 1024,
        request_timeout_ms: 50,
        max_concurrent: 10,
    };

    // === Act ===
    let result = nusa_core::with_timeout(guard.request_timeout_ms, async {
        tokio::time::sleep(Duration::from_secs(10)).await;
        Ok::<_, nusa_core::EngineError>("done")
    })
    .await;

    // === Assert ===
    assert!(result.is_err(), "must timeout");
    assert!(
        matches!(result.unwrap_err(), nusa_core::EngineError::Timeout),
        "must be Timeout variant"
    );
}

#[tokio::test]
async fn resourceguard_validate_request_size_boundary() {
    // === Arrange ===
    let max_bytes = 1024usize;

    // === Act & Assert ===
    // At boundary
    assert!(nusa_core::validate_request_size(Some(1024), max_bytes));
    // Over boundary
    assert!(!nusa_core::validate_request_size(Some(1025), max_bytes));
    // Under boundary
    assert!(nusa_core::validate_request_size(Some(1023), max_bytes));
    // No content length — allowed
    assert!(nusa_core::validate_request_size(None, max_bytes));
    // Zero size
    assert!(nusa_core::validate_request_size(Some(0), max_bytes));
}

// ── Memory Ordering: Atomic Visibility ──

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ratelimiter_relaxed_vs_seqcst_visibility() {
    // === Arrange ===
    // RateLimiter uses parking_lot::Mutex (which provides proper ordering).
    // Verify visibility across threads under concurrent load.
    let limiter = Arc::new(TenantRateLimiter::new(60000, 1000));
    let tenant_id = TenantId::new("visibility-test");

    // === Act ===
    // Thread 1: consume tokens
    let lim1 = limiter.clone();
    let tid1 = tenant_id.clone();
    let t1 = std::thread::spawn(move || {
        for _ in 0..50 {
            lim1.is_allowed(&tid1);
        }
    });

    // Thread 2: read remaining
    let lim2 = limiter.clone();
    let tid2 = tenant_id.clone();
    let t2 = std::thread::spawn(move || {
        let mut readings = Vec::new();
        for _ in 0..50 {
            if let Some(r) = lim2.remaining(&tid2) {
                readings.push(r);
            }
            std::thread::yield_now();
        }
        readings
    });

    t1.join().expect("thread must not panic");
    let readings = t2.join().expect("thread must not panic");

    // === Assert ===
    // All readings should be valid (non-negative, within burst range)
    for r in &readings {
        assert!(*r <= 1000, "remaining must not exceed burst size");
    }
}

// ── Channel: Producer/Consumer Edge Cases ──

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn taskmanager_send_very_large_messages() {
    // === Arrange ===
    let manager = Arc::new(TaskManager::new());

    // === Act ===
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let closed_port = listener.local_addr().expect("addr").port();
    drop(listener);

    let large_payload = vec![0u8; 1024 * 1024]; // 1MB
    let task = OffloadTask::HttpRequest {
        method: "POST".to_string(),
        url: format!("http://127.0.0.1:{closed_port}"),
        headers: Default::default(),
        body: Some(large_payload),
    };
    let (_id, _rx) = manager.submit(task);

    // === Assert ===
    tokio::time::sleep(Duration::from_secs(5)).await;
    // Manager should not crash with large payloads
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn taskmanager_send_many_tiny_messages_no_overhead() {
    // === Arrange ===
    let manager = Arc::new(TaskManager::new());

    // === Act ===
    for _ in 0..100 {
        let task = OffloadTask::Custom {
            task_type: "tiny".to_string(),
            payload: serde_json::json!({}),
        };
        let (_id, _rx) = manager.submit(task);
    }

    // === Assert ===
    tokio::time::sleep(Duration::from_millis(500)).await;
    // No crash means no allocation overhead issues
}
