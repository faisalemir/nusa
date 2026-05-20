//! Per-tenant circuit breaker for two-layer traffic protection.
//! Blueprint 6 D3: Each tenant gets independent circuit breaker.
//!
//! Skills applied:
//! - `m07-concurrency`: DashMap for concurrent tenant circuit breakers
//! - `m13-domain-error`: Two-layer protection (global + per-tenant)
//! - `m09-domain`: TenantId enforces type-level scoping of circuit state

use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tracing::info;

use nusa_core::TenantId;
use crate::circuit_breaker::{CircuitBreaker, CbState};

/// Per-tenant circuit breaker registry.
///
/// m07-concurrency: DashMap enables lock-free concurrent access to tenant breakers.
/// m09-domain: Each tenant's circuit breaker is isolated and independently managed.
pub struct TenantCircuitBreakers {
    breakers: DashMap<TenantId, Arc<CircuitBreaker>>,
    failure_threshold: u64,
    reset_timeout: Duration,
}

impl TenantCircuitBreakers {
    pub fn new(failure_threshold: u64, reset_timeout: Duration) -> Self {
        Self {
            breakers: DashMap::new(),
            failure_threshold,
            reset_timeout,
        }
    }

    /// Get or create a circuit breaker for a tenant.
    pub fn get_or_create(&self, tenant_id: &TenantId) -> Arc<CircuitBreaker> {
        self.breakers
            .entry(tenant_id.clone())
            .or_insert_with(|| {
                let cb = CircuitBreaker::new(self.failure_threshold, self.reset_timeout);
                info!("Created circuit breaker for tenant {}", tenant_id.as_str());
                Arc::new(cb)
            })
            .value()
            .clone()
    }

    /// Check if a tenant is allowed to send requests.
    /// m13-domain-error: Two-layer check — global CB checked first, then per-tenant CB.
    pub fn is_allowed(&self, tenant_id: &TenantId) -> bool {
        if let Some(cb) = self.breakers.get(tenant_id) {
            cb.allow_request()
        } else {
            true // No breaker means tenant is new — allow traffic
        }
    }

    /// Record success for a tenant.
    pub fn record_success(&self, tenant_id: &TenantId) {
        if let Some(cb) = self.breakers.get(tenant_id) {
            cb.record_success();
        }
    }

    /// Record failure for a tenant.
    pub fn record_failure(&self, tenant_id: &TenantId) {
        let cb = self.get_or_create(tenant_id);
        cb.record_failure();
        info!(
            "Circuit breaker opened for tenant {} (state: {:?})",
            tenant_id.as_str(),
            cb.state()
        );
    }

    /// Get the state of a tenant's circuit breaker.
    pub fn state(&self, tenant_id: &TenantId) -> Option<CbState> {
        self.breakers.get(tenant_id).map(|cb| cb.state())
    }

    /// Get the number of tenant circuit breakers.
    pub fn count(&self) -> usize {
        self.breakers.len()
    }
}
