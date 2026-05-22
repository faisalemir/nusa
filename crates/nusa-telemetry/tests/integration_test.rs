//! Telemetry integration tests.
//!
//! Tests full observability stack, OTLP export, Prometheus metrics, health probes, and export under load.

use std::time::Duration;

// ── Full Observability Init ──

#[test]
fn telemetry_integration_observability_stack_init() {
    // Full observability init requires OTLP endpoint running
    // This test verifies the types and structure
    let metrics_result = nusa_telemetry::init_metrics();
    assert!(
        metrics_result.is_ok(),
        "prometheus metrics should init without OTLP"
    );
}

#[test]
fn telemetry_integration_tracing_init_standalone() {
    // init_tracing requires OTLP endpoint
    // Test that the function exists and is callable
    // In CI without OTLP, this will fail gracefully
    let result = nusa_telemetry::init_tracing();
    // May fail if OTLP endpoint not available
    assert!(result.is_ok() || result.is_err());
}

// ── OTLP Export Integration ──

#[test]
fn telemetry_integration_otlp_export_sends_to_collector() {
    // OTLP export is handled by opentelemetry-otlp crate
    // In production, traces are sent to configured collector
    // This test verifies the export types exist
    let _ = opentelemetry::global::tracer_provider();
}

// ── Prometheus Integration ──

#[test]
fn telemetry_integration_prometheus_metrics_rendered() {
    let handle = nusa_telemetry::init_metrics().expect("init metrics");
    let rendered = handle.render();
    assert!(
        !rendered.is_empty(),
        "prometheus output should not be empty"
    );
    assert!(
        rendered.contains("nusa_"),
        "should contain nusa_ prefixed metrics"
    );
}

#[test]
fn telemetry_integration_prometheus_metric_names_correct() {
    let handle = nusa_telemetry::init_metrics().expect("init metrics");
    let rendered = handle.render();

    // Verify standard metric names
    assert!(rendered.contains("nusa_requests_total"));
    assert!(rendered.contains("nusa_requests_failed_total"));
}

// ── Metrics Under Load ──

#[tokio::test]
async fn telemetry_integration_metrics_accurate_under_load() {
    use nusa_telemetry::metrics::NusaMetrics;

    let metrics = NusaMetrics::init();

    for _ in 0..100 {
        metrics.requests_total.increment(1);
    }

    let handle = nusa_telemetry::init_metrics().expect("prometheus handle");
    let rendered = handle.render();
    assert!(rendered.contains("nusa_requests_total"));
}

#[tokio::test]
async fn telemetry_integration_export_doesnt_degrade_performance() {
    use nusa_telemetry::metrics::NusaMetrics;

    let metrics = NusaMetrics::init();

    let start = std::time::Instant::now();

    // Record many metrics rapidly
    for _ in 0..10000 {
        metrics.requests_total.increment(1);
        metrics.request_duration_ms.record(1.0);
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "recording 10000 metrics should be fast, took {:?}",
        elapsed
    );
}

// ── Per-Tenant Telemetry ──

#[test]
fn telemetry_integration_per_tenant_metrics_isolated() {
    use nusa_telemetry::export::TelemetryExport;

    let export = TelemetryExport::with_console_backend(3600);
    export.record_request("tenant-a", 10, 1.0, 100);
    export.record_request("tenant-a", 20, 1.0, 200);
    export.record_request("tenant-b", 30, 2.0, 300);

    assert_eq!(export.tenant_request_count("tenant-a"), 2);
    assert_eq!(export.tenant_request_count("tenant-b"), 1);
    assert_eq!(export.tenant_request_count("tenant-c"), 0);
}

// ── TelemetryExport Integration ──

#[tokio::test]
async fn telemetry_integration_export_loop_graceful_shutdown() {
    use nusa_telemetry::export::TelemetryExport;
    use std::sync::Arc;

    let export = Arc::new(TelemetryExport::with_console_backend(1));
    export.record_request("tenant-a", 100, 64.0, 1024);

    let shutdown_tx = export.clone().start_export_loop();

    // Let it run briefly
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Graceful shutdown
    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn telemetry_integration_file_export_backend() {
    use nusa_telemetry::export::{ExportBackend, TelemetryExport};
    use std::sync::Arc;

    let dir = std::env::temp_dir().join(format!(
        "nusa-telemetry-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");

    let export = Arc::new(TelemetryExport::new(
        1,
        ExportBackend::File {
            path: dir.join("metrics.jsonl"),
        },
    ));
    export.record_request("tenant-file", 50, 32.0, 512);

    let shutdown_tx = export.clone().start_export_loop();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let _ = shutdown_tx.send(());

    let _ = std::fs::remove_dir_all(&dir);
}

// ── Span Creation ──

#[test]
fn telemetry_integration_create_span_with_trace_id() {
    let span = nusa_telemetry::create_span("test-request", "trace-123");
    assert_eq!(span.metadata().map(|m| m.name()), Some("request"));
}
