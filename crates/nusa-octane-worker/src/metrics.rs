//! Worker recycling system with telemetry-driven decisions.
//!
//! Skills applied:
//! - `m07-concurrency`: AtomicU64 for lock-free metrics counters
//! - `m13-domain-error`: Error rate tracking, resource monitoring
//! - `m10-performance`: Efficient telemetry aggregation without locks

use std::sync::atomic::{AtomicU64, Ordering};

/// Worker metrics for telemetry-driven recycling decisions.
#[derive(Debug)]
pub struct WorkerMetrics {
    requests_handled: AtomicU64,
    error_count: AtomicU64,
    avg_response_time_ms: AtomicU64, // Running average
    total_response_time_ms: AtomicU64,
}

impl WorkerMetrics {
    pub fn new() -> Self {
        Self {
            requests_handled: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
            avg_response_time_ms: AtomicU64::new(0),
            total_response_time_ms: AtomicU64::new(0),
        }
    }

    pub fn record_request(&self, duration_ms: u64, success: bool) {
        self.requests_handled.fetch_add(1, Ordering::Relaxed);
        if !success {
            self.error_count.fetch_add(1, Ordering::Relaxed);
        }
        // Update running average
        let total = self
            .total_response_time_ms
            .fetch_add(duration_ms, Ordering::Relaxed)
            + duration_ms;
        let count = self.requests_handled.load(Ordering::Relaxed);
        if let Some(avg) = total.checked_div(count) {
            self.avg_response_time_ms.store(avg, Ordering::Relaxed);
        }
    }

    pub fn requests_handled(&self) -> u64 {
        self.requests_handled.load(Ordering::Relaxed)
    }

    pub fn error_count(&self) -> u64 {
        self.error_count.load(Ordering::Relaxed)
    }

    pub fn error_rate(&self) -> f64 {
        let requests = self.requests_handled.load(Ordering::Relaxed);
        if requests == 0 {
            return 0.0;
        }
        self.error_count.load(Ordering::Relaxed) as f64 / requests as f64
    }

    pub fn avg_response_time_ms(&self) -> u64 {
        self.avg_response_time_ms.load(Ordering::Relaxed)
    }
}

impl Default for WorkerMetrics {
    fn default() -> Self {
        Self::new()
    }
}
