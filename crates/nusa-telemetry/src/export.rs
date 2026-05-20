//! Billing & telemetry export hooks for per-tenant metric aggregation.
//! Blueprint 6 D6: Periodic export of per-tenant metrics to JSON/CSV.
//!
//! Skills applied:
//! - `domain-cloud-native`: S3/Parquet stub, structured billing exports
//! - `m12-lifecycle`: Periodic export task with configurable interval
//! - `m07-concurrency`: Async background export task with cancellation

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde::Serialize;
use tokio::sync::oneshot;
use tracing::info;

/// Billing export record per tenant.
#[derive(Debug, Clone, Serialize)]
pub struct BillingExport {
    pub tenant_id: String,
    pub requests: u64,
    pub cpu_ms: u64,
    pub memory_mb: f64,
    pub bandwidth_bytes: u64,
    pub timestamp: String,
}

/// Telemetry export manager.
///
/// domain-cloud-native: Periodic export with stub for S3/Parquet backend.
/// m12-lifecycle: Configurable export interval, graceful shutdown via oneshot channel.
pub struct TelemetryExport {
    tenant_stats: Arc<Mutex<HashMap<String, TenantMetric>>>,
    export_interval: Duration,
}

#[derive(Debug, Clone, Default)]
struct TenantMetric {
    requests: u64,
    cpu_ms: u64,
    memory_mb: f64,
    bandwidth_bytes: u64,
}

impl TelemetryExport {
    pub fn new(export_interval_secs: u64) -> Self {
        Self {
            tenant_stats: Arc::new(Mutex::new(HashMap::new())),
            export_interval: Duration::from_secs(export_interval_secs),
        }
    }

    /// Record a request metric for a tenant (m07-concurrency).
    pub fn record_request(&self, tenant_id: &str, duration_ms: u64, memory_mb: f64) {
        let mut stats = self.tenant_stats.lock();
        let metric = stats.entry(tenant_id.to_string()).or_default();
        metric.requests += 1;
        metric.cpu_ms += duration_ms;
        metric.memory_mb = metric.memory_mb.max(memory_mb);
        metric.bandwidth_bytes += 0; // Placeholder for actual bandwidth tracking
    }

    /// Start periodic export in background (m12-lifecycle).
    /// Returns a oneshot receiver for graceful shutdown.
    pub fn start_export_loop(self: Arc<Self>) -> oneshot::Sender<()> {
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
        let stats = self.tenant_stats.clone();
        let interval = self.export_interval;

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(interval) => {
                        let exports = Self::generate_exports(&stats);
                        for export in exports {
                            info!("Billing export: {:?}", export);
                            // Stub: send to S3/Parquet in production
                            // Self::upload_to_s3(&export).await.ok();
                        }
                        // Clear stats after export
                        stats.lock().clear();
                    }
                    _ = &mut shutdown_rx => {
                        info!("Telemetry export shutting down");
                        break;
                    }
                }
            }
        });

        shutdown_tx
    }

    fn generate_exports(stats: &Mutex<HashMap<String, TenantMetric>>) -> Vec<BillingExport> {
        let stats = stats.lock();
        let now = chrono::Utc::now().to_rfc3339();

        stats.iter().map(|(tenant_id, metric)| {
            BillingExport {
                tenant_id: tenant_id.clone(),
                requests: metric.requests,
                cpu_ms: metric.cpu_ms,
                memory_mb: metric.memory_mb,
                bandwidth_bytes: metric.bandwidth_bytes,
                timestamp: now.clone(),
            }
        }).collect()
    }
}
