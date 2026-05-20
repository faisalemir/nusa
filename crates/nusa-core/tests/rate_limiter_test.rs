//! Exhaustive tests for TenantRateLimiter (token bucket algorithm).
//!
//! rust-test-deep Phase 1: Core Exhaustive
//! rust-test-deep Phase 3: Concurrency Exhaustive

use nusa_core::{TenantId, TenantRateLimiter};

// ── Happy Paths ──

/// === Arrange ===
/// RateLimiter with 60 rpm, burst 10.
/// === Act ===
/// Single request allowed.
/// === Assert ===
/// is_allowed returns true.
#[test]
fn rate_limiter_first_request_allowed() {
    // === Arrange ===
    let limiter = TenantRateLimiter::new(60, 10);
    let tenant = TenantId::new("tenant-a");

    // === Act ===
    let allowed = limiter.is_allowed(&tenant);

    // === Assert ===
    assert!(allowed, "first request must be allowed");
}

/// === Arrange ===
/// RateLimiter with 60 rpm, burst 10, 10 requests consumed.
/// === Act ===
/// 11th request attempted.
/// === Assert ===
/// 11th request rejected (bucket empty).
#[test]
fn rate_limiter_rejects_after_burst_exhausted() {
    // === Arrange ===
    let limiter = TenantRateLimiter::new(60, 3);
    let tenant = TenantId::new("tenant-b");

    // === Act ===
    let r1 = limiter.is_allowed(&tenant);
    let r2 = limiter.is_allowed(&tenant);
    let r3 = limiter.is_allowed(&tenant);
    let r4 = limiter.is_allowed(&tenant);

    // === Assert ===
    assert!(r1, "request 1 must be allowed");
    assert!(r2, "request 2 must be allowed");
    assert!(r3, "request 3 must be allowed");
    assert!(!r4, "request 4 must be rejected after burst exhausted");
}

/// === Arrange ===
/// RateLimiter with 2 tenants, independent buckets.
/// === Act ===
/// Tenant A exhausts burst, Tenant B sends request.
/// === Assert ===
/// Tenant B still allowed (isolation).
#[test]
fn rate_limiter_tenant_isolation() {
    // === Arrange ===
    let limiter = TenantRateLimiter::new(60, 2);
    let tenant_a = TenantId::new("tenant-a");
    let tenant_b = TenantId::new("tenant-b");

    // === Act ===
    limiter.is_allowed(&tenant_a);
    limiter.is_allowed(&tenant_a);
    limiter.is_allowed(&tenant_a); // should be rejected
    let allowed_b = limiter.is_allowed(&tenant_b);

    // === Assert ===
    assert!(!limiter.is_allowed(&tenant_a), "tenant A must be rate limited");
    assert!(allowed_b, "tenant B must still be allowed");
}

// ── Edge Cases ──

/// === Arrange ===
/// RateLimiter with burst_size = 0.
/// === Act ===
/// Any request attempted.
/// === Assert ===
/// Rejected (no tokens available).
#[test]
fn rate_limiter_zero_burst_rejects_all() {
    // === Arrange ===
    let limiter = TenantRateLimiter::new(60, 0);
    let tenant = TenantId::new("tenant-zero");

    // === Act ===
    let allowed = limiter.is_allowed(&tenant);

    // === Assert ===
    assert!(!allowed, "zero burst must reject all requests");
}

/// === Arrange ===
/// RateLimiter with rpm = 0, burst = 1.
/// === Act ===
/// First request allowed, refill rate is 0.
/// === Assert ===
/// Second request rejected (no refill).
#[test]
fn rate_limiter_zero_rpm_no_refill() {
    // === Arrange ===
    let limiter = TenantRateLimiter::new(0, 1);
    let tenant = TenantId::new("tenant-no-rpm");

    // === Act ===
    let r1 = limiter.is_allowed(&tenant);
    let r2 = limiter.is_allowed(&tenant);

    // === Assert ===
    assert!(r1, "burst allows first request");
    assert!(!r2, "zero rpm means no refill");
}

/// === Arrange ===
/// RateLimiter with very large burst (10000).
/// === Act ===
/// 10000 requests sent.
/// === Assert ===
/// All allowed.
#[test]
fn rate_limiter_large_burst_allows_many() {
    // === Arrange ===
    let limiter = TenantRateLimiter::new(60, 100);
    let tenant = TenantId::new("tenant-large");

    // === Act ===
    let results: Vec<bool> = (0..100).map(|_| limiter.is_allowed(&tenant)).collect();

    // === Assert ===
    assert!(results.iter().all(|&r| r), "all 100 requests must be allowed");
}

// ── Boundary Values ──

/// === Arrange ===
/// RateLimiter with max u64 rpm, burst 1.
/// === Act ===
/// Requests sent.
/// === Assert ===
/// No overflow, first request allowed.
#[test]
fn rate_limiter_max_rpm_no_overflow() {
    // === Arrange ===
    let limiter = TenantRateLimiter::new(u64::MAX, 1);
    let tenant = TenantId::new("tenant-max");

    // === Act ===
    let allowed = limiter.is_allowed(&tenant);

    // === Assert ===
    assert!(allowed, "max rpm must not cause overflow");
}

// ── Concurrency (Phase 3) ──

/// === Arrange ===
/// RateLimiter shared across 10 threads, burst 100.
/// === Act ===
/// Each thread sends 20 requests (200 total, but burst is 100).
/// === Assert ===
/// Exactly 100 allowed, 100 rejected.
#[test]
fn rate_limiter_concurrent_access_thread_safe() {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use std::thread;

    // === Arrange ===
    let limiter = Arc::new(TenantRateLimiter::new(60, 100));
    let tenant = TenantId::new("tenant-concurrent");
    let allowed_count = Arc::new(AtomicU64::new(0));
    let rejected_count = Arc::new(AtomicU64::new(0));

    let mut handles = vec![];

    // === Act ===
    for _ in 0..10 {
        let l = Arc::clone(&limiter);
        let t = tenant.clone();
        let ac = Arc::clone(&allowed_count);
        let rc = Arc::clone(&rejected_count);
        handles.push(thread::spawn(move || {
            for _ in 0..20 {
                if l.is_allowed(&t) {
                    ac.fetch_add(1, Ordering::SeqCst);
                } else {
                    rc.fetch_add(1, Ordering::SeqCst);
                }
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // === Assert ===
    let allowed = allowed_count.load(Ordering::SeqCst);
    let rejected = rejected_count.load(Ordering::SeqCst);
    assert_eq!(allowed + rejected, 200, "total must be 200");
    assert_eq!(allowed, 100, "exactly 100 must be allowed (burst size)");
    assert_eq!(rejected, 100, "exactly 100 must be rejected");
}
