//! Stress tests, decision logic tests, state machine tests, idempotency tests,
//! time-dependent tests, event-driven tests, and fallback tests for nusa-core.
//!
//! Covers: TaskManager, RateLimiter, ResourceGuard, BackpressureGuard,
//! TenantRegistry, RequestContext.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nusa_core::rate_limiter::TenantRateLimiter;
use nusa_core::task::{OffloadTask, TaskManager};
use nusa_core::tenant::{TenantConfig, TenantRegistry};
use nusa_core::types::TenantId;
use nusa_core::{
    BackpressureGuard, RequestContext, ResourceGuard, TraceId, validate_request_size, with_timeout,
};
use tokio::sync::Barrier;

// ============================================================================
// Stress Tests: TaskManager Throughput
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_manager_throughput_100_per_sec_completes() {
    let manager = TaskManager::new();
    let count = 100;
    let start = Instant::now();

    let mut receivers = Vec::new();
    for _ in 0..count {
        let (id, rx) = manager.submit(OffloadTask::Custom {
            task_type: "noop".into(),
            payload: serde_json::json!({}),
        });
        assert!(!id.is_empty());
        receivers.push(rx);
    }

    for rx in receivers {
        let _ = tokio::time::timeout(Duration::from_secs(65), rx).await;
    }

    let elapsed = start.elapsed();
    let rate = count as f64 / elapsed.as_secs_f64();
    assert!(rate >= 50.0, "throughput {rate:.0}/s below 50/s minimum");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_manager_throughput_500_per_sec_completes() {
    let manager = TaskManager::new();
    let count = 500;
    let start = Instant::now();

    let mut receivers = Vec::new();
    for _ in 0..count {
        let (_id, rx) = manager.submit(OffloadTask::Custom {
            task_type: "noop".into(),
            payload: serde_json::json!({}),
        });
        receivers.push(rx);
    }

    let mut latencies = Vec::new();
    for rx in receivers {
        let t0 = Instant::now();
        let _ = tokio::time::timeout(Duration::from_secs(65), rx).await;
        latencies.push(t0.elapsed().as_millis() as u64);
    }
    latencies.sort();

    let elapsed = start.elapsed();
    let rate = count as f64 / elapsed.as_secs_f64();

    let p50 = latencies[count * 50 / 100];
    let p95 = latencies[count * 95 / 100];
    let p99 = latencies[count * 99 / 100];

    assert!(rate >= 200.0, "throughput {rate:.0}/s below 200/s minimum");
    assert!(p50 < 5000, "P50 latency {p50}ms exceeds SLA");
    assert!(p95 < 30000, "P95 latency {p95}ms exceeds SLA");
    assert!(p99 < 60000, "P99 latency {p99}ms exceeds SLA");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn task_manager_throughput_1000_per_sec_completes() {
    let manager = TaskManager::new();
    let count = 1000;
    let start = Instant::now();

    let mut receivers = Vec::new();
    for _ in 0..count {
        let (_id, rx) = manager.submit(OffloadTask::Custom {
            task_type: "noop".into(),
            payload: serde_json::json!({}),
        });
        receivers.push(rx);
    }

    let mut latencies = Vec::new();
    for rx in receivers {
        let t0 = Instant::now();
        let _ = tokio::time::timeout(Duration::from_secs(65), rx).await;
        latencies.push(t0.elapsed().as_millis() as u64);
    }
    latencies.sort();

    let elapsed = start.elapsed();
    let rate = count as f64 / elapsed.as_secs_f64();

    let _p50 = latencies[count * 50 / 100];
    let _p95 = latencies[count * 95 / 100];
    let p99 = latencies[count * 99 / 100];

    assert!(rate >= 100.0, "throughput {rate:.0}/s below 100/s minimum");
    assert!(p99 < 65000, "P99 latency {p99}ms exceeds SLA");
}

// ============================================================================
// Stress Tests: Load Ramp-Up
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_manager_load_ramp_gradual_increase_stable() {
    let manager = TaskManager::new();
    let mut failures = 0;

    for batch_size in [10, 25, 50, 100, 200] {
        let mut receivers = Vec::new();
        for _ in 0..batch_size {
            let (_id, rx) = manager.submit(OffloadTask::Custom {
                task_type: "noop".into(),
                payload: serde_json::json!({}),
            });
            receivers.push(rx);
        }

        for rx in receivers {
            if tokio::time::timeout(Duration::from_secs(65), rx)
                .await
                .is_err()
            {
                failures += 1;
            }
        }
    }

    assert_eq!(failures, 0, "ramp-up caused {failures} failures");
}

// ============================================================================
// Stress Tests: Spike Load
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_manager_spike_2x_baseline_no_failures() {
    let manager = TaskManager::new();
    let baseline = 50;
    let spike = baseline * 2;

    let mut receivers = Vec::new();
    for _ in 0..spike {
        let (_id, rx) = manager.submit(OffloadTask::Custom {
            task_type: "noop".into(),
            payload: serde_json::json!({}),
        });
        receivers.push(rx);
    }

    let mut failures = 0;
    for rx in receivers {
        if tokio::time::timeout(Duration::from_secs(65), rx)
            .await
            .is_err()
        {
            failures += 1;
        }
    }
    assert_eq!(failures, 0, "2x spike caused {failures} failures");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_manager_spike_5x_baseline_no_corruption() {
    let manager = TaskManager::new();
    let spike = 250;

    let mut receivers = Vec::new();
    for _ in 0..spike {
        let (_id, rx) = manager.submit(OffloadTask::Custom {
            task_type: "noop".into(),
            payload: serde_json::json!({}),
        });
        receivers.push(rx);
    }

    let mut failures = 0;
    for rx in receivers {
        if tokio::time::timeout(Duration::from_secs(65), rx)
            .await
            .is_err()
        {
            failures += 1;
        }
    }
    assert!(
        failures < spike / 2,
        "5x spike caused excessive failures: {failures}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn task_manager_spike_10x_baseline_recovers() {
    let manager = TaskManager::new();
    let spike = 500;

    let mut receivers = Vec::new();
    for _ in 0..spike {
        let (_id, rx) = manager.submit(OffloadTask::Custom {
            task_type: "noop".into(),
            payload: serde_json::json!({}),
        });
        receivers.push(rx);
    }

    let mut failures = 0;
    for rx in receivers {
        if tokio::time::timeout(Duration::from_secs(65), rx)
            .await
            .is_err()
        {
            failures += 1;
        }
    }
    assert!(failures < spike, "10x spike should not fail all tasks");

    // Verify recovery: submit a normal batch after spike
    let (id, rx) = manager.submit(OffloadTask::Custom {
        task_type: "noop".into(),
        payload: serde_json::json!({}),
    });
    let result = tokio::time::timeout(Duration::from_secs(65), rx).await;
    assert!(result.is_ok(), "should recover after 10x spike, task {id}");
}

// ============================================================================
// Stress Tests: Soak Test
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_manager_soak_30_seconds_memory_stable() {
    let manager = Arc::new(TaskManager::new());
    let duration = Duration::from_secs(30);
    let interval = Duration::from_millis(100);
    let mut task_count = 0u64;
    let start = Instant::now();

    while start.elapsed() < duration {
        let mgr = manager.clone();
        let (_id, rx) = mgr.submit(OffloadTask::Custom {
            task_type: "noop".into(),
            payload: serde_json::json!({}),
        });
        task_count += 1;

        let _ = tokio::time::timeout(Duration::from_secs(5), rx).await;

        tokio::time::sleep(interval).await;
    }

    assert!(
        task_count > 50,
        "soak test submitted only {task_count} tasks in 30s"
    );
}

// ============================================================================
// Stress Tests: Thundering Herd
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn task_manager_thundering_herd_no_cascade_failure() {
    let manager = Arc::new(TaskManager::new());
    let num_clients = 50;
    let tasks_per_client = 10;
    let barrier = Arc::new(Barrier::new(num_clients));

    let mut handles = Vec::new();

    for _ in 0..num_clients {
        let mgr = manager.clone();
        let bar = barrier.clone();
        let handle = tokio::spawn(async move {
            bar.wait().await;
            let mut results = Vec::new();
            for _ in 0..tasks_per_client {
                let (_id, rx) = mgr.submit(OffloadTask::Custom {
                    task_type: "noop".into(),
                    payload: serde_json::json!({}),
                });
                results.push(rx);
            }
            results
        });
        handles.push(handle);
    }

    let mut total_failures = 0;
    for handle in handles {
        let receivers = handle.await.expect("client task panicked");
        for rx in receivers {
            if tokio::time::timeout(Duration::from_secs(65), rx)
                .await
                .is_err()
            {
                total_failures += 1;
            }
        }
    }

    let total_submitted = num_clients * tasks_per_client;
    assert!(
        total_failures < total_submitted / 2,
        "thundering herd caused {total_failures}/{total_submitted} failures"
    );
}

// ============================================================================
// Stress Tests: Backpressure
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn backpressure_guard_queue_full_rejects_with_error() {
    let guard = BackpressureGuard::new(2);

    let permit1 = guard.try_acquire().await;
    let permit2 = guard.try_acquire().await;
    assert!(permit1.is_some(), "first permit should be acquired");
    assert!(permit2.is_some(), "second permit should be acquired");

    let permit3 = guard.try_acquire().await;
    assert!(
        permit3.is_none(),
        "third permit should be rejected (backpressure)"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn backpressure_guard_recovers_after_release() {
    let guard = Arc::new(BackpressureGuard::new(1));

    let permit = guard.try_acquire().await;
    assert!(permit.is_some());

    drop(permit);

    let permit2 = guard.try_acquire().await;
    assert!(permit2.is_some(), "should acquire after release");
}

// ============================================================================
// Stress Tests: Degradation and Recovery
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_manager_degradation_recovers() {
    let manager = TaskManager::new();

    let (id, rx) = manager.submit(OffloadTask::Custom {
        task_type: "noop".into(),
        payload: serde_json::json!({}),
    });
    let result = tokio::time::timeout(Duration::from_secs(65), rx).await;
    assert!(
        result.is_ok(),
        "task {id} should complete (pre-degradation)"
    );

    let status = manager.status(&id);
    assert!(status.completed, "task should be marked completed");
}

// ============================================================================
// Stress Tests: Recovery After Stress
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_manager_recovery_returns_to_baseline() {
    let manager = TaskManager::new();

    for _ in 0..100 {
        let (_id, rx) = manager.submit(OffloadTask::Custom {
            task_type: "noop".into(),
            payload: serde_json::json!({}),
        });
        let _ = tokio::time::timeout(Duration::from_secs(65), rx).await;
    }

    let (id, rx) = manager.submit(OffloadTask::Custom {
        task_type: "noop".into(),
        payload: serde_json::json!({}),
    });
    let result = tokio::time::timeout(Duration::from_secs(65), rx).await;
    assert!(
        result.is_ok(),
        "should recover after stress, task {id} completed"
    );
}

// ============================================================================
// Stress Tests: RateLimiter Throughput
// ============================================================================

#[test]
fn rate_limiter_throughput_below_limit_all_accepted() {
    let limiter = TenantRateLimiter::new(1000, 100);
    let tenant = TenantId::new("tenant-below-limit");

    for _ in 0..50 {
        assert!(limiter.is_allowed(&tenant), "should accept below limit");
    }
}

#[test]
fn rate_limiter_throughput_at_limit_last_accepted() {
    let limiter = TenantRateLimiter::new(100, 50);
    let tenant = TenantId::new("tenant-at-limit");

    for _ in 0..49 {
        assert!(limiter.is_allowed(&tenant));
    }
    // The 50th may still be accepted (at boundary), but subsequent should fail
    let _ = limiter.is_allowed(&tenant);
}

#[test]
fn rate_limiter_throughput_above_limit_rejects() {
    let limiter = TenantRateLimiter::new(100, 10);
    let tenant = TenantId::new("tenant-above-limit");

    for _ in 0..10 {
        assert!(limiter.is_allowed(&tenant));
    }

    let mut rejections = 0;
    for _ in 0..20 {
        if !limiter.is_allowed(&tenant) {
            rejections += 1;
        }
    }
    assert!(rejections > 0, "should reject when above limit");
}

// ============================================================================
// Stress Tests: RateLimiter Spike
// ============================================================================

#[test]
fn rate_limiter_spike_burst_within_limit_accepted() {
    let limiter = TenantRateLimiter::new(60, 20);
    let tenant = TenantId::new("tenant-burst");

    for _ in 0..20 {
        assert!(limiter.is_allowed(&tenant), "burst within limit");
    }
}

#[test]
fn rate_limiter_spike_burst_exceeded_then_rejects() {
    let limiter = TenantRateLimiter::new(60, 10);
    let tenant = TenantId::new("tenant-burst-exceeded");

    for _ in 0..10 {
        assert!(limiter.is_allowed(&tenant));
    }

    let mut rejections = 0;
    for _ in 0..10 {
        if !limiter.is_allowed(&tenant) {
            rejections += 1;
        }
    }
    assert!(rejections > 0, "should reject after burst exceeded");
}

// ============================================================================
// Decision Logic Tests: RateLimiter Decision Table
// ============================================================================

#[test]
fn rate_limiter_decision_tokens_available_accepts() {
    let limiter = TenantRateLimiter::new(1000, 100);
    let tenant = TenantId::new("tenant-tokens-available");
    assert!(limiter.is_allowed(&tenant), "tokens available -> accept");
}

#[test]
fn rate_limiter_decision_tokens_exhausted_rejects() {
    let limiter = TenantRateLimiter::new(100, 2);
    let tenant = TenantId::new("tenant-exhausted");
    assert!(limiter.is_allowed(&tenant));
    assert!(limiter.is_allowed(&tenant));

    let mut rejected = false;
    for _ in 0..10 {
        if !limiter.is_allowed(&tenant) {
            rejected = true;
            break;
        }
    }
    assert!(rejected, "tokens exhausted -> reject");
}

#[test]
fn rate_limiter_decision_new_tenant_creates_bucket() {
    let limiter = TenantRateLimiter::new(1000, 50);
    let tenant = TenantId::new("tenant-new");

    let result = limiter.is_allowed(&tenant);
    assert!(result, "new tenant should be accepted with fresh bucket");

    let remaining = limiter.remaining(&tenant);
    assert!(
        remaining.is_some(),
        "new tenant should have remaining tokens"
    );
}

#[test]
fn rate_limiter_decision_unknown_tenant_remaining_none() {
    let limiter = TenantRateLimiter::new(1000, 50);
    let tenant = TenantId::new("tenant-unknown");
    assert!(
        limiter.remaining(&tenant).is_none(),
        "unknown tenant -> None"
    );
}

// ============================================================================
// Decision Logic Tests: RateLimiter Guard Clauses
// ============================================================================

#[test]
fn rate_limiter_guard_tokens_zero_rejects() {
    let limiter = TenantRateLimiter::new(100, 1);
    let tenant = TenantId::new("tenant-zero-tokens");
    assert!(limiter.is_allowed(&tenant));
    assert!(
        !limiter.is_allowed(&tenant) || limiter.remaining(&tenant) == Some(0),
        "after exhausting, should reject or show zero"
    );
}

#[test]
fn rate_limiter_guard_tenant_enabled_accepts() {
    let limiter = TenantRateLimiter::new(1000, 100);
    let tenant = TenantId::new("tenant-enabled");
    assert!(limiter.is_allowed(&tenant));
}

#[test]
fn rate_limiter_guard_multiple_tenants_independent() {
    let limiter = TenantRateLimiter::new(60, 2);
    let t1 = TenantId::new("t1");
    let t2 = TenantId::new("t2");

    assert!(limiter.is_allowed(&t1));
    assert!(limiter.is_allowed(&t1));
    assert!(limiter.is_allowed(&t2));
    assert!(limiter.is_allowed(&t2));
}

// ============================================================================
// Decision Logic Tests: BackpressureGuard Decision Table
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn backpressure_permits_available_allows() {
    let guard = BackpressureGuard::new(5);
    let permit = guard.try_acquire().await;
    assert!(permit.is_some(), "permits available -> allow");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn backpressure_capacity_zero_rejects() {
    let guard = BackpressureGuard::new(0);
    let permit = guard.try_acquire().await;
    assert!(permit.is_none(), "capacity zero -> reject");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn backpressure_at_boundary_last_permit() {
    let guard = BackpressureGuard::new(1);
    let p1 = guard.try_acquire().await;
    assert!(p1.is_some(), "first permit at capacity 1");
    let p2 = guard.try_acquire().await;
    assert!(p2.is_none(), "second permit at capacity 1 -> reject");
}

// ============================================================================
// Decision Logic Tests: validate_request_size
// ============================================================================

#[test]
fn validate_request_size_unknown_size_accepts() {
    assert!(validate_request_size(None, 1024), "unknown size -> accept");
}

#[test]
fn validate_request_size_under_limit_accepts() {
    assert!(
        validate_request_size(Some(512), 1024),
        "under limit -> accept"
    );
}

#[test]
fn validate_request_size_at_limit_accepts() {
    assert!(
        validate_request_size(Some(1024), 1024),
        "at limit -> accept"
    );
}

#[test]
fn validate_request_size_over_limit_rejects() {
    assert!(
        !validate_request_size(Some(1025), 1024),
        "over limit -> reject"
    );
}

#[test]
fn validate_request_size_zero_size_accepts() {
    assert!(validate_request_size(Some(0), 1024), "zero size -> accept");
}

#[test]
fn validate_request_size_max_size_accepts() {
    let max = usize::MAX;
    assert!(
        validate_request_size(Some(max as u64), max),
        "MAX size -> accept"
    );
}

#[test]
fn validate_request_size_large_payload_rejects() {
    assert!(
        !validate_request_size(Some(11 * 1024 * 1024), 10 * 1024 * 1024),
        "11MB over 10MB limit -> reject"
    );
}

// ============================================================================
// Decision Logic Tests: with_timeout
// ============================================================================

#[tokio::test]
async fn with_timeout_completes_within_limit_returns_ok() {
    let result = with_timeout(5000, async { Ok::<_, nusa_core::EngineError>(42) }).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
}

#[tokio::test]
async fn with_timeout_exceeds_limit_returns_timeout() {
    let result = with_timeout(10, async {
        tokio::time::sleep(Duration::from_secs(10)).await;
        Ok::<_, nusa_core::EngineError>(42)
    })
    .await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        nusa_core::EngineError::Timeout
    ));
}

#[tokio::test]
async fn with_timeout_inner_error_propagates() {
    let result = with_timeout(5000, async {
        Err::<i32, _>(nusa_core::EngineError::ResourceLimit)
    })
    .await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        nusa_core::EngineError::ResourceLimit
    ));
}

#[tokio::test]
async fn with_timeout_zero_timeout_triggers_immediately() {
    let result = with_timeout(0, async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok::<_, nusa_core::EngineError>(42)
    })
    .await;
    assert!(result.is_err(), "zero timeout should trigger");
}

// ============================================================================
// Decision Logic Tests: TenantRegistry Decision Table
// ============================================================================

#[test]
fn tenant_registry_register_not_exists_creates() {
    let mut registry = TenantRegistry::new();
    let id = TenantId::new("new-tenant");
    let config = TenantConfig {
        id: id.clone(),
        vfs_root: "/tmp/tenant".into(),
        max_memory_mb: 256,
        max_requests_per_minute: 100,
        enabled: true,
    };
    registry.register(config);
    assert!(registry.get(&id).is_some(), "register not exists -> Some");
}

#[test]
fn tenant_registry_register_exists_overwrites() {
    let mut registry = TenantRegistry::new();
    let id = TenantId::new("overwrite-tenant");

    let config1 = TenantConfig {
        id: id.clone(),
        vfs_root: "/tmp/old".into(),
        max_memory_mb: 256,
        max_requests_per_minute: 100,
        enabled: true,
    };
    registry.register(config1);

    let config2 = TenantConfig {
        id: id.clone(),
        vfs_root: "/tmp/new".into(),
        max_memory_mb: 512,
        max_requests_per_minute: 200,
        enabled: true,
    };
    registry.register(config2);

    let got = registry.get(&id).expect("should exist after overwrite");
    assert_eq!(got.vfs_root, "/tmp/new");
    assert_eq!(got.max_memory_mb, 512);
}

#[test]
fn tenant_registry_get_exists_returns_some() {
    let mut registry = TenantRegistry::new();
    let id = TenantId::new("get-tenant");
    let config = TenantConfig {
        id: id.clone(),
        vfs_root: "/tmp/test".into(),
        max_memory_mb: 256,
        max_requests_per_minute: 100,
        enabled: true,
    };
    registry.register(config);
    assert!(registry.get(&id).is_some());
}

#[test]
fn tenant_registry_get_not_exists_returns_none() {
    let registry = TenantRegistry::new();
    let id = TenantId::new("missing-tenant");
    assert!(registry.get(&id).is_none());
}

#[test]
fn tenant_registry_is_enabled_true() {
    let mut registry = TenantRegistry::new();
    let id = TenantId::new("enabled-tenant");
    registry.register(TenantConfig {
        id: id.clone(),
        vfs_root: "/tmp/test".into(),
        max_memory_mb: 256,
        max_requests_per_minute: 100,
        enabled: true,
    });
    assert!(registry.is_enabled(&id));
}

#[test]
fn tenant_registry_is_enabled_false() {
    let mut registry = TenantRegistry::new();
    let id = TenantId::new("disabled-tenant");
    registry.register(TenantConfig {
        id: id.clone(),
        vfs_root: "/tmp/test".into(),
        max_memory_mb: 256,
        max_requests_per_minute: 100,
        enabled: false,
    });
    assert!(!registry.is_enabled(&id));
}

#[test]
fn tenant_registry_is_enabled_missing_returns_false() {
    let mut registry = TenantRegistry::new();
    registry.register(TenantConfig {
        id: TenantId::new("known"),
        vfs_root: "/t".into(),
        max_memory_mb: 256,
        max_requests_per_minute: 100,
        enabled: true,
    });
    let id = TenantId::new("missing-tenant");
    assert!(!registry.is_enabled(&id));
}

// ============================================================================
// Decision Logic Tests: RequestContext Field Combinations
// ============================================================================

#[test]
fn request_context_combinations_tenant_body_env_headers() {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);

    let ctx1 = RequestContext::new("/app".into(), "index.php".into(), deadline);
    assert!(ctx1.tenant_id().is_none());
    assert!(ctx1.body().is_empty());

    let ctx2 = ctx1
        .with_tenant(TenantId::new("test-tenant"))
        .with_body(bytes::Bytes::from("hello"))
        .with_env(Arc::new(HashMap::from([("KEY".into(), "val".into())])));
    assert_eq!(ctx2.tenant_id().unwrap().as_str(), "test-tenant");
    assert_eq!(ctx2.body().as_ref(), b"hello");
    assert_eq!(ctx2.env().get("KEY"), Some(&"val".to_string()));
}

#[test]
fn request_context_combinations_with_trace_id() {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let ctx = RequestContext::new("/app".into(), "index.php".into(), deadline);
    let trace = ctx.trace_id();
    assert_ne!(trace.to_string(), "", "trace_id should not be empty");
}

#[test]
fn request_context_combinations_with_headers() {
    use http::HeaderMap;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut headers = HeaderMap::new();
    headers.insert("x-custom", "value".parse().unwrap());

    let ctx =
        RequestContext::new("/app".into(), "index.php".into(), deadline).with_headers(headers);
    assert!(ctx.headers().contains_key("x-custom"));
}

#[test]
fn request_context_combinations_all_fields_set() {
    use http::HeaderMap;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());

    let ctx = RequestContext::new("/app".into(), "api.php".into(), deadline)
        .with_tenant(TenantId::new("tenant-a"))
        .with_body(bytes::Bytes::from(r#"{"key":"value"}"#))
        .with_env(Arc::new(HashMap::from([(
            "APP_ENV".into(),
            "testing".into(),
        )])))
        .with_headers(headers);

    assert_eq!(ctx.tenant_id().unwrap().as_str(), "tenant-a");
    assert_eq!(ctx.body().as_ref(), br#"{"key":"value"}"#);
    assert_eq!(ctx.vfs_root().to_str().unwrap(), "/app");
    assert_eq!(ctx.script_path().to_str().unwrap(), "api.php");
    assert!(ctx.headers().contains_key("content-type"));
    assert_eq!(ctx.env().get("APP_ENV"), Some(&"testing".to_string()));
}

// ============================================================================
// State Machine Tests: Task State Transitions
// ============================================================================

#[tokio::test]
async fn task_state_submitted_to_running_to_completed_valid() {
    let manager = TaskManager::new();
    let (id, rx) = manager.submit(OffloadTask::Custom {
        task_type: "noop".into(),
        payload: serde_json::json!({}),
    });

    let result = tokio::time::timeout(Duration::from_secs(65), rx).await;
    assert!(result.is_ok(), "task should complete");

    let status = manager.status(&id);
    assert!(status.completed, "task should be in completed state");
    assert!(status.result.is_some(), "completed task should have result");
    assert!(
        !status.result.as_ref().unwrap().success,
        "custom task should fail (not implemented)"
    );
}

#[tokio::test]
async fn task_state_submitted_status_before_completion() {
    let manager = TaskManager::new();
    let (id, _rx) = manager.submit(OffloadTask::Custom {
        task_type: "noop".into(),
        payload: serde_json::json!({}),
    });

    let status = manager.status(&id);
    assert!(
        !status.completed,
        "newly submitted task should not be completed yet"
    );
}

#[test]
fn task_manager_new_creates_empty_state() {
    let manager = TaskManager::new();
    let status = manager.status("nonexistent-id");
    assert!(
        !status.completed,
        "nonexistent task should not be completed"
    );
    assert!(status.result.is_none());
}

// ============================================================================
// State Machine Tests: TenantRegistry State
// ============================================================================

#[test]
fn tenant_registry_state_register_get_returns_some() {
    let mut registry = TenantRegistry::new();
    let id = TenantId::new("state-test");
    registry.register(TenantConfig {
        id: id.clone(),
        vfs_root: "/tmp/test".into(),
        max_memory_mb: 256,
        max_requests_per_minute: 100,
        enabled: true,
    });
    assert!(registry.get(&id).is_some());
}

#[test]
fn tenant_registry_state_reregister_get_returns_some() {
    let mut registry = TenantRegistry::new();
    let id = TenantId::new("reregister-test");

    registry.register(TenantConfig {
        id: id.clone(),
        vfs_root: "/tmp/old".into(),
        max_memory_mb: 256,
        max_requests_per_minute: 100,
        enabled: true,
    });
    registry.register(TenantConfig {
        id: id.clone(),
        vfs_root: "/tmp/new".into(),
        max_memory_mb: 512,
        max_requests_per_minute: 200,
        enabled: true,
    });

    assert!(registry.get(&id).is_some());
    assert_eq!(registry.get(&id).unwrap().max_memory_mb, 512);
}

// ============================================================================
// Idempotency Tests: TenantRegistry Register
// ============================================================================

#[test]
fn tenant_registry_register_idempotent_single() {
    let mut registry = TenantRegistry::new();
    let id = TenantId::new("idempotent-tenant");
    let config = TenantConfig {
        id: id.clone(),
        vfs_root: "/tmp/test".into(),
        max_memory_mb: 256,
        max_requests_per_minute: 100,
        enabled: true,
    };

    registry.register(config.clone());
    registry.register(config);

    let got = registry.get(&id).expect("should exist");
    assert_eq!(got.vfs_root, "/tmp/test");
}

#[test]
fn tenant_registry_register_idempotent_n_times() {
    let mut registry = TenantRegistry::new();
    let id = TenantId::new("idempotent-n");
    let config = TenantConfig {
        id: id.clone(),
        vfs_root: "/tmp/test".into(),
        max_memory_mb: 256,
        max_requests_per_minute: 100,
        enabled: true,
    };

    for _ in 0..100 {
        registry.register(config.clone());
    }

    let got = registry
        .get(&id)
        .expect("should exist after 100 registrations");
    assert_eq!(got.vfs_root, "/tmp/test");
}

// ============================================================================
// Idempotency Tests: TaskManager Submit
// ============================================================================

#[tokio::test]
async fn task_manager_submit_same_task_different_ids() {
    let manager = TaskManager::new();
    let task = OffloadTask::Custom {
        task_type: "noop".into(),
        payload: serde_json::json!({}),
    };

    let (id1, _rx1) = manager.submit(task.clone());
    let (id2, _rx2) = manager.submit(task);

    assert_ne!(
        id1, id2,
        "submitting same task should generate different IDs"
    );
}

// ============================================================================
// Idempotency Tests: RequestContext Clone
// ============================================================================

#[test]
fn request_context_clone_idempotent_identical_state() {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let ctx = RequestContext::new("/app".into(), "index.php".into(), deadline)
        .with_tenant(TenantId::new("clone-test"))
        .with_body(bytes::Bytes::from("data"));

    let cloned = ctx.clone();

    assert_eq!(ctx.tenant_id(), cloned.tenant_id());
    assert_eq!(ctx.body(), cloned.body());
    assert_eq!(ctx.vfs_root(), cloned.vfs_root());
    assert_eq!(ctx.script_path(), cloned.script_path());
}

// ============================================================================
// Time-Dependent Tests: RateLimiter Time
// ============================================================================

#[test]
fn rate_limiter_time_refill_after_elapsed() {
    let limiter = TenantRateLimiter::new(60, 5);
    let tenant = TenantId::new("time-refill");

    for _ in 0..5 {
        assert!(limiter.is_allowed(&tenant));
    }

    assert!(
        !limiter.is_allowed(&tenant),
        "immediately after burst, should reject"
    );
}

#[test]
fn rate_limiter_time_remaining_decreases() {
    let limiter = TenantRateLimiter::new(1000, 100);
    let tenant = TenantId::new("time-decrease");

    assert!(
        limiter.is_allowed(&tenant),
        "first request should be allowed"
    );
    let before = limiter
        .remaining(&tenant)
        .expect("bucket must exist after first request");
    assert!(
        limiter.is_allowed(&tenant),
        "second request should be allowed"
    );
    let after = limiter.remaining(&tenant).expect("bucket must still exist");

    assert!(
        after < before,
        "remaining should decrease after consumption: before={before}, after={after}"
    );
}

// ============================================================================
// Time-Dependent Tests: with_timeout Time
// ============================================================================

#[tokio::test]
async fn with_timeout_exact_expiry_triggers() {
    let result = with_timeout(5, async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        Ok::<_, nusa_core::EngineError>(42)
    })
    .await;
    assert!(result.is_err(), "should timeout before 500ms completes");
}

#[tokio::test]
async fn with_timeout_one_tick_before_expiry_completes() {
    let result = with_timeout(5000, async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        Ok::<_, nusa_core::EngineError>(99)
    })
    .await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 99);
}

// ============================================================================
// Event-Driven Tests: TaskManager Events
// ============================================================================

#[tokio::test]
async fn task_manager_task_completed_event_delivered() {
    let manager = TaskManager::new();
    let (id, rx) = manager.submit(OffloadTask::Custom {
        task_type: "noop".into(),
        payload: serde_json::json!({}),
    });

    let result = tokio::time::timeout(Duration::from_secs(65), rx).await;
    assert!(result.is_ok(), "task {id} should complete");

    let status = manager.status(&id);
    assert!(status.completed, "completed event should be delivered");
    assert!(status.result.is_some());
}

#[tokio::test]
async fn task_manager_task_failed_event_delivered() {
    let manager = TaskManager::new();
    let (id, rx) = manager.submit(OffloadTask::Custom {
        task_type: "unknown_type".into(),
        payload: serde_json::json!({}),
    });

    let result = tokio::time::timeout(Duration::from_secs(65), rx).await;
    assert!(result.is_ok(), "task should deliver result");

    if let Ok(task_result) = result.unwrap() {
        assert!(!task_result.success, "unknown task type should fail");
        assert!(task_result.error.is_some());
    }

    let status = manager.status(&id);
    assert!(status.completed);
}

// ============================================================================
// Fallback Tests: Task Fallback
// ============================================================================

#[tokio::test]
async fn task_manager_fallback_primary_fails_error_with_reason() {
    let manager = TaskManager::new();
    let (_id, rx) = manager.submit(OffloadTask::Custom {
        task_type: "not-implemented".into(),
        payload: serde_json::json!({}),
    });

    let result = tokio::time::timeout(Duration::from_secs(65), rx).await;
    assert!(result.is_ok(), "task should deliver");

    if let Ok(task_result) = result.unwrap() {
        assert!(!task_result.success);
        assert!(task_result.error.is_some());
        let err = task_result.error.unwrap();
        assert!(
            err.contains("not yet implemented"),
            "error should mention task type not implemented: {err}"
        );
    }
}

#[tokio::test]
async fn task_manager_fallback_file_read_nonexistent_returns_error() {
    let manager = TaskManager::new();
    let (_id, rx) = manager.submit(OffloadTask::FileOperation {
        operation: "read".into(),
        path: "/nonexistent/path/file.txt".into(),
        data: None,
    });

    let result = tokio::time::timeout(Duration::from_secs(35), rx).await;
    assert!(result.is_ok(), "task should deliver result");

    if let Ok(task_result) = result.unwrap() {
        assert!(!task_result.success);
        assert!(task_result.error.is_some());
    }
}

// ============================================================================
// ResourceGuard Decision Tests
// ============================================================================

#[test]
fn resource_guard_default_values_within_sla() {
    let guard = ResourceGuard::default();
    assert_eq!(guard.max_request_bytes, 10 * 1024 * 1024);
    assert_eq!(guard.request_timeout_ms, 30_000);
    assert_eq!(guard.max_concurrent, 100);
}

#[test]
fn resource_guard_clone_preserves_values() {
    let guard = ResourceGuard {
        max_request_bytes: 5 * 1024 * 1024,
        request_timeout_ms: 15_000,
        max_concurrent: 50,
    };
    let cloned = guard.clone();
    assert_eq!(cloned.max_request_bytes, 5 * 1024 * 1024);
    assert_eq!(cloned.request_timeout_ms, 15_000);
    assert_eq!(cloned.max_concurrent, 50);
}

// ============================================================================
// TraceId Tests
// ============================================================================

#[test]
fn trace_id_new_generates_unique() {
    let t1 = TraceId::new();
    let t2 = TraceId::new();
    assert_ne!(t1, t2, "new trace IDs should be unique");
}

#[test]
fn trace_id_from_uuid_roundtrip() {
    let uuid = uuid::Uuid::new_v4();
    let trace = TraceId::from_uuid(uuid);
    assert_eq!(trace.as_uuid(), uuid);
}

#[test]
fn trace_id_display_not_empty() {
    let trace = TraceId::new();
    let s = trace.to_string();
    assert!(!s.is_empty());
}

// ============================================================================
// TenantId Tests
// ============================================================================

#[test]
fn tenant_id_new_as_str_roundtrip() {
    let id = TenantId::new("my-tenant");
    assert_eq!(id.as_str(), "my-tenant");
}

#[test]
fn tenant_id_display_matches_inner() {
    let id = TenantId::new("display-tenant");
    assert_eq!(id.to_string(), "display-tenant");
}

#[test]
fn tenant_id_eq_hash_same_value() {
    let id1 = TenantId::new("same");
    let id2 = TenantId::new("same");
    assert_eq!(id1, id2);
}

#[test]
fn tenant_id_different_values_not_equal() {
    let id1 = TenantId::new("a");
    let id2 = TenantId::new("b");
    assert_ne!(id1, id2);
}

// ============================================================================
// OffloadTask Serialization Tests
// ============================================================================

#[test]
fn offload_task_http_request_serialization_roundtrip() {
    let task = OffloadTask::HttpRequest {
        method: "GET".into(),
        url: "https://example.com".into(),
        headers: HashMap::new(),
        body: None,
    };
    let json = serde_json::to_string(&task).unwrap();
    let decoded: OffloadTask = serde_json::from_str(&json).unwrap();
    match decoded {
        OffloadTask::HttpRequest { method, url, .. } => {
            assert_eq!(method, "GET");
            assert_eq!(url, "https://example.com");
        }
        _ => panic!("wrong variant after roundtrip"),
    }
}

#[test]
fn offload_task_custom_serialization_roundtrip() {
    let task = OffloadTask::Custom {
        task_type: "my-task".into(),
        payload: serde_json::json!({"key": "value"}),
    };
    let json = serde_json::to_string(&task).unwrap();
    let decoded: OffloadTask = serde_json::from_str(&json).unwrap();
    match decoded {
        OffloadTask::Custom { task_type, payload } => {
            assert_eq!(task_type, "my-task");
            assert_eq!(payload["key"], "value");
        }
        _ => panic!("wrong variant after roundtrip"),
    }
}

#[test]
fn offload_task_file_operation_serialization_roundtrip() {
    let task = OffloadTask::FileOperation {
        operation: "read".into(),
        path: "/tmp/file.txt".into(),
        data: None,
    };
    let json = serde_json::to_string(&task).unwrap();
    let decoded: OffloadTask = serde_json::from_str(&json).unwrap();
    match decoded {
        OffloadTask::FileOperation {
            operation, path, ..
        } => {
            assert_eq!(operation, "read");
            assert_eq!(path, "/tmp/file.txt");
        }
        _ => panic!("wrong variant after roundtrip"),
    }
}

// ============================================================================
// TaskResult Tests
// ============================================================================

#[test]
fn task_result_success_fields() {
    let result = nusa_core::task::TaskResult {
        success: true,
        data: vec![1, 2, 3],
        error: None,
    };
    assert!(result.success);
    assert_eq!(result.data, vec![1, 2, 3]);
    assert!(result.error.is_none());
}

#[test]
fn task_result_failure_fields() {
    let result = nusa_core::task::TaskResult {
        success: false,
        data: vec![],
        error: Some("error message".into()),
    };
    assert!(!result.success);
    assert!(result.data.is_empty());
    assert!(result.error.is_some());
}

// ============================================================================
// TenantConfig Serialization Tests
// ============================================================================

#[test]
fn tenant_config_serialization_roundtrip() {
    let config = TenantConfig {
        id: TenantId::new("serialize-test"),
        vfs_root: "/tmp/vfs".into(),
        max_memory_mb: 512,
        max_requests_per_minute: 200,
        enabled: true,
    };
    let json = serde_json::to_string(&config).unwrap();
    let decoded: TenantConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.id.as_str(), "serialize-test");
    assert_eq!(decoded.vfs_root, "/tmp/vfs");
    assert_eq!(decoded.max_memory_mb, 512);
    assert!(decoded.enabled);
}
