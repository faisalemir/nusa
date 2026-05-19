//! Observability: OpenTelemetry tracing, Prometheus metrics, structured JSON logging.
//!
//! Skills applied:
//! - `domain-cloud-native`: /metrics endpoint, OTLP export, JSON logs
//! - `m10-performance`: Batch OTLP exporter, global Prometheus recorder
//! - `m07-concurrency`: Global recorder setup once

use std::sync::atomic::{AtomicU64, Ordering};

use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize the full observability stack.
///
/// Sets up:
/// 1. OpenTelemetry tracing with OTLP exporter
/// 2. Prometheus metrics recorder
/// 3. Structured JSON logging with env-filter
pub fn init() -> anyhow::Result<()> {
    init_tracing()?;
    init_metrics()?;
    Ok(())
}

/// Initialize OpenTelemetry tracing with OTLP exporter.
///
/// Uses HTTP binary protocol (port 4318) which doesn't require gRPC/tonic.
pub fn init_tracing() -> anyhow::Result<()> {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .build()?;

    let tracer_provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .build();

    let tracer = tracer_provider.tracer("phprt-runtime");

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer().json())
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .init();

    global::set_tracer_provider(tracer_provider);

    Ok(())
}

/// Initialize Prometheus metrics recorder.
pub fn init_metrics() -> anyhow::Result<()> {
    let _recorder = metrics_exporter_prometheus::PrometheusBuilder::new()
        .install_recorder()?;

    tracing::info!("Prometheus metrics initialized");
    Ok(())
}

/// Helper to create a span with trace context.
#[allow(dead_code)]
pub fn create_span(_name: &str, trace_id: &str) -> tracing::Span {
    tracing::info_span!("request", trace_id = %trace_id)
}

/// Runtime metrics for the gateway.
pub struct NusaMetrics {
    pub requests_total: AtomicU64,
    pub errors_total: AtomicU64,
    pub active_requests: AtomicU64,
}

impl NusaMetrics {
    pub fn new() -> Self {
        Self {
            requests_total: AtomicU64::new(0),
            errors_total: AtomicU64::new(0),
            active_requests: AtomicU64::new(0),
        }
    }

    pub fn record_request(&self) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_error(&self) {
        self.errors_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn request_started(&self) {
        self.active_requests.fetch_add(1, Ordering::Relaxed);
    }

    pub fn request_finished(&self) {
        self.active_requests.fetch_sub(1, Ordering::Relaxed);
    }
}

impl Default for NusaMetrics {
    fn default() -> Self {
        Self::new()
    }
}
