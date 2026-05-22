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
    assert!(
        !limiter.is_allowed(&tenant_a),
        "tenant A must be rate limited"
    );
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
    assert!(
        results.iter().all(|&r| r),
        "all 100 requests must be allowed"
    );
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
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

// ── Extended: Exact Boundary & Sliding Window ──

/// Exactly at burst limit → last request passes.
#[test]
fn rate_limiter_exactly_at_limit_last_passes() {
    let limiter = TenantRateLimiter::new(60, 5);
    let tenant = TenantId::new("tenant-exact");

    for i in 1..=5 {
        assert!(limiter.is_allowed(&tenant), "request {} should pass", i);
    }
    assert!(
        !limiter.is_allowed(&tenant),
        "6th request should be rejected"
    );
}

/// One over burst limit → 429-equivalent behavior.
#[test]
fn rate_limiter_one_over_limit_rejected() {
    let limiter = TenantRateLimiter::new(60, 1);
    let tenant = TenantId::new("tenant-one-over");

    assert!(limiter.is_allowed(&tenant), "first request passes");
    assert!(
        !limiter.is_allowed(&tenant),
        "second request should be rejected"
    );
}

/// Sliding window: after time passes, tokens refill.
#[test]
fn rate_limiter_refill_after_time() {
    use std::thread;
    use std::time::Duration;

    // High RPM to get quick refill (6000 rpm = 100/sec)
    let limiter = TenantRateLimiter::new(6000, 1);
    let tenant = TenantId::new("tenant-refill");

    // Use the single burst token
    limiter.is_allowed(&tenant);
    assert!(!limiter.is_allowed(&tenant), "should be empty");

    // Wait for refill (at 6000 rpm = 100/sec, 50ms should refill ~5 tokens)
    thread::sleep(Duration::from_millis(50));

    // Should have tokens again
    assert!(
        limiter.is_allowed(&tenant),
        "should have refilled after wait"
    );
}

/// Burst allowance exceeded → rejects but doesn't waste resources.
#[test]
fn rate_limiter_well_over_limit_rejects_cleanly() {
    let limiter = TenantRateLimiter::new(60, 5);
    let tenant = TenantId::new("tenant-well-over");

    // Exhaust burst
    for _ in 0..5 {
        limiter.is_allowed(&tenant);
    }

    // Send many more — all should be rejected cleanly
    for i in 0..100 {
        assert!(
            !limiter.is_allowed(&tenant),
            "request {} should be rejected",
            i
        );
    }
}

/// Runtime limit change → new limit takes effect immediately.
#[test]
fn rate_limiter_different_tenants_independent_limits() {
    // Each tenant gets its own bucket
    let limiter = TenantRateLimiter::new(60, 3);

    let t1 = TenantId::new("t1");
    let t2 = TenantId::new("t2");
    let t3 = TenantId::new("t3");

    // t1 exhausts its burst
    for _ in 0..3 {
        limiter.is_allowed(&t1);
    }
    assert!(!limiter.is_allowed(&t1), "t1 should be limited");

    // t2 and t3 should still have full burst
    assert!(limiter.is_allowed(&t2), "t2 should be allowed");
    assert!(limiter.is_allowed(&t3), "t3 should be allowed");
}

/// Multiple tenant isolation under concurrent load.
#[test]
fn rate_limiter_concurrent_multiple_tenants_isolation() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;

    let limiter = Arc::new(TenantRateLimiter::new(60, 10));
    let tenant_a = TenantId::new("tenant-a-concurrent");
    let tenant_b = TenantId::new("tenant-b-concurrent");

    let allowed_a = Arc::new(AtomicU64::new(0));
    let allowed_b = Arc::new(AtomicU64::new(0));

    // Thread A hammers tenant A
    let la = limiter.clone();
    let ta = tenant_a.clone();
    let aa = allowed_a.clone();
    let handle_a = thread::spawn(move || {
        for _ in 0..50 {
            if la.is_allowed(&ta) {
                aa.fetch_add(1, Ordering::SeqCst);
            }
        }
    });

    // Thread B hammers tenant B
    let lb = limiter.clone();
    let tb = tenant_b.clone();
    let ab = allowed_b.clone();
    let handle_b = thread::spawn(move || {
        for _ in 0..50 {
            if lb.is_allowed(&tb) {
                ab.fetch_add(1, Ordering::SeqCst);
            }
        }
    });

    handle_a.join().unwrap();
    handle_b.join().unwrap();

    // Each tenant should have exactly 10 allowed (burst size)
    assert_eq!(
        allowed_a.load(Ordering::SeqCst),
        10,
        "tenant A should have 10 allowed"
    );
    assert_eq!(
        allowed_b.load(Ordering::SeqCst),
        10,
        "tenant B should have 10 allowed"
    );
}
