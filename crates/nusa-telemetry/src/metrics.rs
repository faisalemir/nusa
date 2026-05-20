//! Prometheus metrics definitions for the Nusa runtime.
//!
//! Skills applied:
//! - `m10-performance`: Counters, histograms, gauges for observability
//! - `domain-cloud-native`: Standard Prometheus metric naming

use metrics::{Counter, Gauge, Histogram};

/// Shared metric handles, initialized once and reused.
pub struct NusaMetrics {
    // Counters
    pub requests_total: Counter,
    pub requests_failed_total: Counter,
    pub tenants_active: Gauge,

    // Histograms
    pub request_duration_ms: Histogram,
    pub ipc_latency_ms: Histogram,

    // Gauges
    pub worker_pool_size: Gauge,
    pub worker_rss_mb: Gauge,
    pub backpressure_usage: Gauge,
}

impl NusaMetrics {
    /// Initialize all metrics with standard Prometheus naming.
    pub fn init() -> Self {
        Self {
            requests_total: metrics::counter!("nusa_requests_total"),
            requests_failed_total: metrics::counter!("nusa_requests_failed_total"),
            tenants_active: metrics::gauge!("nusa_tenants_active"),

            request_duration_ms: metrics::histogram!("nusa_request_duration_ms"),
            ipc_latency_ms: metrics::histogram!("nusa_ipc_latency_ms"),

            worker_pool_size: metrics::gauge!("nusa_worker_pool_size"),
            worker_rss_mb: metrics::gauge!("nusa_worker_rss_mb"),
            backpressure_usage: metrics::gauge!("nusa_backpressure_usage"),
        }
    }
}

/// Per-tenant metrics helpers (D5).
/// Uses owned String keys to avoid lifetime issues with the metrics macro.
pub mod tenant {
    use metrics::counter;
    use metrics::histogram;

    /// Increment request counter for a specific tenant.
    pub fn record_request(tenant_id: &str) {
        let key = format!("nusa_requests_total{{tenant_id=\"{}\"}}", tenant_id);
        counter!(key).increment(1);
    }

    /// Increment failure counter for a specific tenant.
    pub fn record_failure(tenant_id: &str) {
        let key = format!("nusa_requests_failed_total{{tenant_id=\"{}\"}}", tenant_id);
        counter!(key).increment(1);
    }

    /// Record request duration for a specific tenant.
    pub fn record_duration(tenant_id: &str, duration_ms: f64) {
        let key = format!("nusa_request_duration_ms{{tenant_id=\"{}\"}}", tenant_id);
        histogram!(key).record(duration_ms);
    }
}
