//! Health and readiness probe handlers.
//!
//! Skills applied:
//! - `domain-cloud-native`: /health and /ready endpoints for Kubernetes/SRE
//! - `m09-domain`: Health state as domain model
//! - `m07-concurrency`: AtomicU64/AtomicBool for lock-free counters

use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};

/// Shared health state for the gateway.
pub struct HealthState {
    /// Whether the application has completed startup.
    ready: AtomicBool,
    /// Total successful requests processed.
    success_count: AtomicU64,
    /// Total errors encountered.
    error_count: AtomicU64,
}

impl HealthState {
    pub fn new() -> Self {
        Self {
            ready: AtomicBool::new(false),
            success_count: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
        }
    }

    pub fn mark_ready(&self) {
        self.ready.store(true, Ordering::SeqCst);
    }

    pub fn record_success(&self) {
        self.success_count.fetch_add(1, Ordering::SeqCst);
    }

    pub fn record_error(&self) {
        self.error_count.fetch_add(1, Ordering::SeqCst);
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    pub fn success_count(&self) -> u64 {
        self.success_count.load(Ordering::SeqCst)
    }

    pub fn error_count(&self) -> u64 {
        self.error_count.load(Ordering::SeqCst)
    }
}

impl Default for HealthState {
    fn default() -> Self {
        Self::new()
    }
}

/// GET /health — Always returns OK if the server is running.
pub async fn health_handler() -> &'static str {
    "OK"
}
