//! Billing & telemetry export hooks for per-tenant metric aggregation.
//! Blueprint 6 D6: Periodic export of per-tenant metrics to JSON/CSV.
//!
//! Skills applied:
//! - `domain-cloud-native`: S3/Parquet stub, structured billing exports
//! - `m12-lifecycle`: Periodic export task with configurable interval
//! - `m07-concurrency`: Async background export task with cancellation
//! - `m04-zero-cost`: enum-based export backend abstraction

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tracing::{info, warn};

/// Billing export record per tenant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingExport {
    pub tenant_id: String,
    pub requests: u64,
    pub cpu_ms: u64,
    pub memory_mb: f64,
    pub bandwidth_bytes: u64,
    pub timestamp: String,
}

/// Export backend variant (m05-type-driven: enum avoids dyn async trait issues).
#[derive(Clone)]
pub enum ExportBackend {
    /// Console backend — logs each record.
    Console,
    /// File backend — appends JSON lines to a file.
    File { path: std::path::PathBuf },
}

impl ExportBackend {
    /// Export billing records to the backend.
    pub async fn export(&self, records: &[BillingExport]) -> anyhow::Result<()> {
        match self {
            ExportBackend::Console => {
                for record in records {
                    info!(
                        tenant_id = %record.tenant_id,
                        requests = record.requests,
                        cpu_ms = record.cpu_ms,
                        memory_mb = record.memory_mb,
                        bandwidth_bytes = record.bandwidth_bytes,
                        "Billing export"
                    );
                }
                Ok(())
            }
            ExportBackend::File { path } => {
                if records.is_empty() {
                    return Ok(());
                }

                let mut file = tokio::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .await?;

                for record in records {
                    let line = serde_json::to_string(record)?;
                    tokio::io::AsyncWriteExt::write_all(&mut file, line.as_bytes()).await?;
                    tokio::io::AsyncWriteExt::write_all(&mut file, b"\n").await?;
                }

                info!(
                    path = ?path,
                    count = records.len(),
                    "Exported billing records to file"
                );
                Ok(())
            }
        }
    }

    /// Returns the backend name for logging/debugging.
    pub fn name(&self) -> &'static str {
        match self {
            ExportBackend::Console => "console",
            ExportBackend::File { .. } => "file",
        }
    }
}

/// Telemetry export manager.
///
/// domain-cloud-native: Periodic export with pluggable backend.
/// m12-lifecycle: Configurable export interval, graceful shutdown via oneshot channel.
pub struct TelemetryExport {
    tenant_stats: Arc<Mutex<HashMap<String, TenantMetric>>>,
    export_interval: Duration,
    backend: ExportBackend,
}

#[derive(Debug, Clone, Default)]
struct TenantMetric {
    requests: u64,
    cpu_ms: u64,
    memory_mb: f64,
    bandwidth_bytes: u64,
}

impl TelemetryExport {
    /// Create a new TelemetryExport with the given backend.
    pub fn new(export_interval_secs: u64, backend: ExportBackend) -> Self {
        Self {
            tenant_stats: Arc::new(Mutex::new(HashMap::new())),
            export_interval: Duration::from_secs(export_interval_secs),
            backend,
        }
    }

    /// Create a TelemetryExport with the default console backend.
    pub fn with_console_backend(export_interval_secs: u64) -> Self {
        Self::new(export_interval_secs, ExportBackend::Console)
    }

    /// Request count for a tenant (test / diagnostics).
    #[doc(hidden)]
    pub fn tenant_request_count(&self, tenant_id: &str) -> u64 {
        self.tenant_stats
            .lock()
            .get(tenant_id)
            .map(|m| m.requests)
            .unwrap_or(0)
    }

    /// Record a request metric for a tenant (m07-concurrency).
    pub fn record_request(
        &self,
        tenant_id: &str,
        duration_ms: u64,
        memory_mb: f64,
        response_bytes: u64,
    ) {
        let mut stats = self.tenant_stats.lock();
        let metric = stats.entry(tenant_id.to_string()).or_default();
        metric.requests += 1;
        metric.cpu_ms += duration_ms;
        metric.memory_mb = metric.memory_mb.max(memory_mb);
        metric.bandwidth_bytes += response_bytes;
    }

    /// Start periodic export in background (m12-lifecycle).
    /// Returns a oneshot receiver for graceful shutdown.
    pub fn start_export_loop(self: Arc<Self>) -> oneshot::Sender<()> {
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
        let stats = self.tenant_stats.clone();
        let interval = self.export_interval;
        let backend = self.backend.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(interval) => {
                        let exports = Self::generate_exports(&stats);
                        if !exports.is_empty()
                            && let Err(e) = backend.export(&exports).await
                        {
                            warn!(backend = backend.name(), err = %e, "Telemetry export failed");
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

        stats
            .iter()
            .map(|(tenant_id, metric)| BillingExport {
                tenant_id: tenant_id.clone(),
                requests: metric.requests,
                cpu_ms: metric.cpu_ms,
                memory_mb: metric.memory_mb,
                bandwidth_bytes: metric.bandwidth_bytes,
                timestamp: now.clone(),
            })
            .collect()
    }
}
