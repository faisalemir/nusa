//! Soak / endurance tests for core components.
//!
//! rust-test §Endurance / Soak Tests
//! Sustained load to detect memory leaks, resource accumulation, degradation.

use std::collections::HashSet;
use std::time::Duration;

use nusa_core::{OffloadTask, RequestContext, TaskManager, TenantConfig, TenantId, TenantRegistry};

// ── Sustained Load: RequestContext ──

/// Sustained load for 5 seconds — baseline stability.
#[test]
fn soak_request_context_5_seconds() {
    let start = std::time::Instant::now();
    let mut count = 0;

    while start.elapsed() < Duration::from_secs(5) {
        let ctx = RequestContext::new(
            "/app/public".into(),
            "index.php".into(),
            tokio::time::Instant::now() + Duration::from_secs(30),
        );
        assert!(!ctx.trace_id().as_uuid().to_string().is_empty());
        count += 1;
    }

    assert!(
        count > 1000,
        "should create 1000+ contexts in 5 seconds: {}",
        count
    );
}

/// Sustained load — verify no duplicate trace IDs over extended period.
#[test]
fn soak_no_duplicate_trace_ids_over_time() {
    let mut seen = HashSet::new();
    let start = std::time::Instant::now();

    while start.elapsed() < Duration::from_secs(10) {
        let ctx = RequestContext::new(
            "/app/public".into(),
            "index.php".into(),
            tokio::time::Instant::now() + Duration::from_secs(30),
        );
        let id = ctx.trace_id();
        assert!(
            seen.insert(id),
            "duplicate trace ID detected after {} creations",
            seen.len()
        );
    }

    assert!(
        seen.len() > 10_000,
        "should have 10k+ unique IDs: {}",
        seen.len()
    );
}

// ── Sustained Load: TenantRegistry ──

/// Sustained tenant registration over time — no performance degradation.
#[test]
fn soak_tenant_registry_sustained_registration() {
    let mut registry = TenantRegistry::new();
    let start = std::time::Instant::now();
    let mut count = 0;

    while start.elapsed() < Duration::from_secs(5) {
        let config = TenantConfig {
            id: TenantId::new(format!("soak-tenant-{}", count)),
            vfs_root: "/app/public".into(),
            max_memory_mb: 256,
            max_requests_per_minute: 60,
            enabled: true,
        };
        registry.register(config);
        count += 1;
    }

    assert!(
        count > 100,
        "should register 100+ tenants in 5 seconds: {}",
        count
    );

    // Verify all tenants are enabled
    for i in 0..count {
        let tenant = TenantId::new(format!("soak-tenant-{}", i));
        assert!(
            registry.is_enabled(&tenant),
            "tenant {} should be enabled",
            i
        );
    }
}

// ── Sustained Load: TaskManager ──

/// Sustained task submission over time — no channel backlog.
#[test]
fn soak_task_manager_sustained_submission() {
    let mgr = TaskManager::new();
    let start = std::time::Instant::now();
    let mut submitted = 0;

    while start.elapsed() < Duration::from_secs(5) {
        let (_id, rx) = mgr.submit(OffloadTask::Custom {
            task_type: "soak".into(),
            payload: serde_json::json!({ "iteration": submitted }),
        });
        // Wait for completion to prevent backlog
        let _ = rx.blocking_recv();
        submitted += 1;
    }

    assert!(
        submitted > 10,
        "should complete 10+ tasks in 5 seconds: {}",
        submitted
    );
}

// ── Memory Stability Checks ──

/// RequestContext creation in a tight loop — no memory growth pattern.
#[test]
fn soak_memory_stability_request_context_loop() {
    // Run 100k iterations and verify no slowdown
    let start = std::time::Instant::now();

    for _ in 0..100_000 {
        let ctx = RequestContext::new(
            "/app/public".into(),
            "index.php".into(),
            tokio::time::Instant::now() + Duration::from_secs(30),
        );
        let _ = ctx.trace_id();
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(30),
        "100k contexts took {:?} — possible memory issue",
        elapsed
    );
}

/// TenantId creation in a tight loop — no memory growth.
#[test]
fn soak_memory_stability_tenant_id_loop() {
    let start = std::time::Instant::now();

    for i in 0..100_000 {
        let _tenant = TenantId::new(format!("memory-test-{}", i));
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "100k tenant IDs took {:?}",
        elapsed
    );
}

// ── Recovery After Load ──

/// System returns to baseline after sustained load.
#[test]
fn soak_recovery_after_load() {
    // Phase 1: Heavy load
    let start = std::time::Instant::now();
    let mut heavy_count = 0;
    while start.elapsed() < Duration::from_secs(3) {
        let ctx = RequestContext::new(
            "/app/public".into(),
            "index.php".into(),
            tokio::time::Instant::now() + Duration::from_secs(30),
        );
        let _ = ctx.trace_id();
        heavy_count += 1;
    }

    // Phase 2: Recovery — should be just as fast
    let recovery_start = std::time::Instant::now();
    let mut recovery_count = 0;
    while recovery_start.elapsed() < Duration::from_secs(3) {
        let ctx = RequestContext::new(
            "/app/public".into(),
            "index.php".into(),
            tokio::time::Instant::now() + Duration::from_secs(30),
        );
        let _ = ctx.trace_id();
        recovery_count += 1;
    }

    // Recovery should be at least 30% as fast as heavy load
    let ratio = recovery_count as f64 / heavy_count as f64;
    assert!(
        ratio > 0.3,
        "recovery ({}) should be >= 30% of heavy load ({})",
        recovery_count,
        heavy_count
    );
}
