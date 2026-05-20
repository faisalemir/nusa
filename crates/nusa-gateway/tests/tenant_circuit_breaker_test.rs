//! Exhaustive tests for TenantCircuitBreakers.
//!
//! rust-test-deep Phase 1: Core Exhaustive
//! rust-test-deep Phase 3: Concurrency Exhaustive

use nusa_core::TenantId;
use nusa_gateway::tenant_circuit_breaker::TenantCircuitBreakers;
use std::time::Duration;

// ── Happy Paths ──

/// === Arrange ===
/// TenantCB with threshold 3, timeout 30s.
/// === Act ===
/// First request for new tenant.
/// === Assert ===
/// Allowed (no breaker yet).
#[test]
fn tenant_cb_new_tenant_allowed() {
    // === Arrange ===
    let cb = TenantCircuitBreakers::new(3, Duration::from_secs(30));
    let tenant = TenantId::new("acme");

    // === Act ===
    let allowed = cb.is_allowed(&tenant);

    // === Assert ===
    assert!(allowed, "new tenant must be allowed");
}

/// === Arrange ===
/// TenantCB with threshold 2.
/// === Act ===
/// Record 2 failures for tenant.
/// === Assert ===
/// Tenant CB opens, requests rejected.
#[test]
fn tenant_cb_opens_after_threshold() {
    // === Arrange ===
    let cb = TenantCircuitBreakers::new(2, Duration::from_secs(30));
    let tenant = TenantId::new("acme");

    // === Act ===
    cb.record_failure(&tenant);
    cb.record_failure(&tenant);

    // === Assert ===
    assert!(!cb.is_allowed(&tenant), "tenant CB must be open after threshold");
}

/// === Arrange ===
/// TenantCB with 2 tenants, threshold 2.
/// === Act ===
/// Tenant A trips breaker, Tenant B sends requests.
/// === Assert ===
/// Tenant B still allowed (isolation).
#[test]
fn tenant_cb_isolation_between_tenants() {
    // === Arrange ===
    let cb = TenantCircuitBreakers::new(2, Duration::from_secs(30));
    let tenant_a = TenantId::new("tenant-a");
    let tenant_b = TenantId::new("tenant-b");

    // === Act ===
    cb.record_failure(&tenant_a);
    cb.record_failure(&tenant_a);

    // === Assert ===
    assert!(!cb.is_allowed(&tenant_a), "tenant A must be blocked");
    assert!(cb.is_allowed(&tenant_b), "tenant B must still be allowed");
}

/// === Arrange ===
/// TenantCB, tenant trips breaker, then records success after timeout.
/// === Act ===
/// Wait for timeout, record success.
/// === Assert ===
/// CB closes.
#[test]
fn tenant_cb_recovers_after_timeout() {
    // === Arrange ===
    let cb = TenantCircuitBreakers::new(1, Duration::from_millis(50));
    let tenant = TenantId::new("acme");

    cb.record_failure(&tenant);
    assert!(!cb.is_allowed(&tenant), "must be open");

    // === Act ===
    std::thread::sleep(Duration::from_millis(60));

    // === Assert ===
    assert!(cb.is_allowed(&tenant), "must be half-open after timeout");
}

// ── Edge Cases ──

/// === Arrange ===
/// TenantCB with threshold 0.
/// === Act ===
/// Record failure.
/// === Assert ===
/// Breaker opens immediately.
#[test]
fn tenant_cb_zero_threshold_opens_immediately() {
    // === Arrange ===
    let cb = TenantCircuitBreakers::new(0, Duration::from_secs(30));
    let tenant = TenantId::new("acme");

    // === Act ===
    cb.record_failure(&tenant);

    // === Assert ===
    assert!(!cb.is_allowed(&tenant), "zero threshold must open immediately");
}

/// === Arrange ===
/// TenantCB, check state for non-existent tenant.
/// === Act ===
/// Query state.
/// === Assert ===
/// Returns None.
#[test]
fn tenant_cb_state_for_unknown_tenant() {
    // === Arrange ===
    let cb = TenantCircuitBreakers::new(3, Duration::from_secs(30));
    let tenant = TenantId::new("unknown");

    // === Act ===
    let state = cb.state(&tenant);

    // === Assert ===
    assert!(state.is_none(), "unknown tenant has no CB state");
}
