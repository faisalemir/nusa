//! Tenant rate limiter using token bucket algorithm.
//!
//! Skills applied:
//! - `m05-type-driven`: TenantId newtype enforces per-tenant scoping
//! - `m13-domain-error`: Returns 429 when bucket exhausted

use std::collections::HashMap;
use std::time::Instant;

use parking_lot::Mutex;

use crate::types::TenantId;

/// Token bucket rate limiter for a single tenant.
struct Bucket {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64,      // tokens per second
    last_refill: Instant,
}

impl Bucket {
    fn new(max_requests_per_minute: u64, burst_size: u64) -> Self {
        let now = Instant::now();
        let burst = burst_size as f64;
        let refill_rate = max_requests_per_minute as f64 / 60.0;

        Self {
            tokens: burst,
            max_tokens: burst,
            refill_rate,
            last_refill: now,
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        if elapsed > 0.0 {
            self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
            self.last_refill = now;
        }
    }

    fn try_consume(&mut self) -> bool {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Per-tenant rate limiter.
/// D2: Returns 429 Too Many Requests when a tenant's bucket is empty.
pub struct TenantRateLimiter {
    buckets: Mutex<HashMap<TenantId, Bucket>>,
    max_requests_per_minute: u64,
    burst_size: u64,
}

impl TenantRateLimiter {
    pub fn new(max_requests_per_minute: u64, burst_size: u64) -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
            max_requests_per_minute,
            burst_size,
        }
    }

    /// Check if a tenant has available rate. Returns true if allowed.
    pub fn is_allowed(&self, tenant_id: &TenantId) -> bool {
        let mut buckets = self.buckets.lock();
        let bucket = buckets
            .entry(tenant_id.clone())
            .or_insert_with(|| Bucket::new(self.max_requests_per_minute, self.burst_size));
        bucket.try_consume()
    }

    /// Get remaining tokens for a tenant (for 429 header info).
    pub fn remaining(&self, tenant_id: &TenantId) -> Option<u64> {
        let buckets = self.buckets.lock();
        buckets.get(tenant_id).map(|b| b.tokens.floor() as u64)
    }
}
