//! Observability: OpenTelemetry tracing, Prometheus metrics, structured JSON logging.
//!
//! Skills applied:
//! - `domain-cloud-native`: /metrics endpoint, OTLP export, JSON logs
//! - `m10-performance`: Batch OTLP exporter, global Prometheus recorder
//! - `m07-concurrency`: Global recorder setup once

#![deny(unsafe_code)]
#![warn(clippy::all)]
#![allow(missing_docs)]

pub mod export;
pub mod metrics;

use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Full observability initialization result.
pub struct Observability {
    pub prometheus_handle: metrics_exporter_prometheus::PrometheusHandle,
}

/// Initialize the full observability stack.
///
/// Sets up:
/// 1. OpenTelemetry tracing with OTLP exporter
/// 2. Prometheus metrics recorder (metrics crate)
/// 3. Structured JSON logging with env-filter
pub fn init() -> anyhow::Result<Observability> {
    let prometheus_handle = init_metrics()?;
    init_tracing()?;
    Ok(Observability { prometheus_handle })
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

    let tracer = tracer_provider.tracer("nusa-runtime");

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer().json())
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .init();

    global::set_tracer_provider(tracer_provider);

    Ok(())
}

/// Initialize Prometheus metrics recorder using the `metrics` crate.
///
/// Returns a PrometheusHandle that can be used to render the metrics
/// in Prometheus exposition format via the /metrics endpoint.
pub fn init_metrics() -> anyhow::Result<metrics_exporter_prometheus::PrometheusHandle> {
    let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().install_recorder()?;

    // Pre-initialize all metric handles so they're ready to use
    let _ = metrics::NusaMetrics::init();

    tracing::info!("Prometheus metrics initialized (counters + histograms + gauges)");
    Ok(recorder)
}

/// Create a span with trace context for request tracing.
pub fn create_span(_name: &str, trace_id: &str) -> tracing::Span {
    tracing::info_span!("request", trace_id = %trace_id)
}
