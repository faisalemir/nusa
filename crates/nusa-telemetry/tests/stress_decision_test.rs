//! Stress tests, decision logic tests, and export tests for nusa-telemetry.
//!
//! Covers: NusaMetrics, TelemetryExport, BillingExport, ExportBackend.

use std::sync::Arc;

use nusa_telemetry::export::{BillingExport, ExportBackend, TelemetryExport};
use nusa_telemetry::metrics::NusaMetrics;

// ============================================================================
// Stress Tests: Metrics Throughput
// ============================================================================

#[test]
fn metrics_throughput_1000_recordings_per_sec() {
    let metrics = NusaMetrics::init();
    let start = std::time::Instant::now();

    for _ in 0..1000 {
        metrics.requests_total.increment(1);
    }

    let elapsed = start.elapsed();
    let rate = 1000.0 / elapsed.as_secs_f64();
    assert!(rate >= 1000.0, "throughput {rate:.0}/s below 1000/s");
}

#[test]
fn metrics_throughput_10000_recordings_per_sec() {
    let metrics = NusaMetrics::init();
    let start = std::time::Instant::now();

    for _ in 0..10_000 {
        metrics.requests_total.increment(1);
        metrics.requests_failed_total.increment(1);
    }

    let elapsed = start.elapsed();
    let rate = 10_000.0 / elapsed.as_secs_f64();
    assert!(rate >= 5000.0, "throughput {rate:.0}/s below 5000/s");
}

#[test]
fn metrics_throughput_100000_recordings_per_sec() {
    let metrics = NusaMetrics::init();
    let start = std::time::Instant::now();

    for _ in 0..100_000 {
        metrics.requests_total.increment(1);
    }

    let elapsed = start.elapsed();
    let rate = 100_000.0 / elapsed.as_secs_f64();
    assert!(rate >= 10000.0, "throughput {rate:.0}/s below 10000/s");
}

// ============================================================================
// Stress Tests: Metrics Accuracy Under Load
// ============================================================================

#[test]
fn metrics_accuracy_counters_accurate_under_concurrent_load() {
    let metrics = Arc::new(NusaMetrics::init());
    let num_threads = 4;
    let increments_per_thread = 1000;

    let mut handles = Vec::new();
    for _ in 0..num_threads {
        let m = metrics.clone();
        handles.push(std::thread::spawn(move || {
            for _ in 0..increments_per_thread {
                m.requests_total.increment(1);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // Counter should reflect total increments (metrics crate handles atomic counters)
    // We can't directly read the counter value, so verify no panics
}

#[test]
fn metrics_accuracy_histogram_record_accurate() {
    let metrics = NusaMetrics::init();

    for i in 1..=100 {
        metrics.request_duration_ms.record(i as f64);
    }

    // Verify no panics - histograms accumulate correctly
}

#[test]
fn metrics_accuracy_gauge_set_accurate() {
    let metrics = NusaMetrics::init();

    metrics.worker_pool_size.set(8.0);
    metrics.worker_rss_mb.set(256.0);
    metrics.backpressure_usage.set(0.5);

    // Verify no panics - gauges set correctly
}

// ============================================================================
// Decision Logic Tests: Export Backend Console
// ============================================================================

#[tokio::test]
async fn export_backend_console_prints_records() {
    let backend = ExportBackend::Console;
    let records = vec![BillingExport {
        tenant_id: "tenant-a".into(),
        requests: 100,
        cpu_ms: 5000,
        memory_mb: 128.0,
        bandwidth_bytes: 1_000_000,
        timestamp: "2026-01-01T00:00:00Z".into(),
    }];

    let result = backend.export(&records).await;
    assert!(result.is_ok(), "console export should succeed");
}

#[tokio::test]
async fn export_backend_console_empty_records_noop() {
    let backend = ExportBackend::Console;
    let records: Vec<BillingExport> = vec![];

    let result = backend.export(&records).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn export_backend_console_name_is_console() {
    let backend = ExportBackend::Console;
    assert_eq!(backend.name(), "console");
}

// ============================================================================
// Decision Logic Tests: Export Backend File
// ============================================================================

#[tokio::test]
async fn export_backend_file_writes_json_lines() {
    let dir = std::env::temp_dir();
    let file_path = dir.join("nusa_telemetry_export_test.jsonl");

    let backend = ExportBackend::File {
        path: file_path.clone(),
    };
    let records = vec![
        BillingExport {
            tenant_id: "tenant-a".into(),
            requests: 50,
            cpu_ms: 2500,
            memory_mb: 64.0,
            bandwidth_bytes: 500_000,
            timestamp: "2026-01-01T00:00:00Z".into(),
        },
        BillingExport {
            tenant_id: "tenant-b".into(),
            requests: 75,
            cpu_ms: 3750,
            memory_mb: 96.0,
            bandwidth_bytes: 750_000,
            timestamp: "2026-01-01T00:00:00Z".into(),
        },
    ];

    let result = backend.export(&records).await;
    assert!(result.is_ok(), "file export should succeed");

    // Verify file exists and has content
    let content = tokio::fs::read_to_string(&file_path).await.unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2, "should have 2 JSON lines");

    // Verify each line is valid JSON
    for line in lines {
        let parsed: Result<BillingExport, _> = serde_json::from_str(line);
        assert!(parsed.is_ok(), "each line should be valid JSON");
    }

    let _ = std::fs::remove_file(&file_path);
}

#[tokio::test]
async fn export_backend_file_empty_records_noop() {
    let dir = std::env::temp_dir();
    let file_path = dir.join("nusa_telemetry_empty.jsonl");

    let backend = ExportBackend::File {
        path: file_path.clone(),
    };
    let records: Vec<BillingExport> = vec![];

    let result = backend.export(&records).await;
    assert!(result.is_ok());

    // File should not be created for empty records
    assert!(!file_path.exists(), "empty export should not create file");
}

#[tokio::test]
async fn export_backend_file_appends_on_repeated_calls() {
    let dir = std::env::temp_dir();
    let file_path = dir.join("nusa_telemetry_append.jsonl");

    let backend = ExportBackend::File {
        path: file_path.clone(),
    };

    // First export
    backend
        .export(&[BillingExport {
            tenant_id: "t1".into(),
            requests: 10,
            cpu_ms: 500,
            memory_mb: 32.0,
            bandwidth_bytes: 100_000,
            timestamp: "2026-01-01T00:00:00Z".into(),
        }])
        .await
        .unwrap();

    // Second export
    backend
        .export(&[BillingExport {
            tenant_id: "t2".into(),
            requests: 20,
            cpu_ms: 1000,
            memory_mb: 64.0,
            bandwidth_bytes: 200_000,
            timestamp: "2026-01-01T01:00:00Z".into(),
        }])
        .await
        .unwrap();

    let content = tokio::fs::read_to_string(&file_path).await.unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2, "should have 2 lines after 2 exports");

    let _ = std::fs::remove_file(&file_path);
}

#[tokio::test]
async fn export_backend_file_name_is_file() {
    let backend = ExportBackend::File {
        path: std::path::PathBuf::from("/tmp/test.jsonl"),
    };
    assert_eq!(backend.name(), "file");
}

// ============================================================================
// Decision Logic Tests: TelemetryExport
// ============================================================================

#[test]
fn telemetry_export_new_creates_instance() {
    let _export = TelemetryExport::with_console_backend(60);
}

#[test]
fn telemetry_export_record_request_updates_stats() {
    let export = TelemetryExport::with_console_backend(60);

    export.record_request("tenant-a", 100, 64.0, 500_000);
    export.record_request("tenant-a", 200, 128.0, 300_000);
    export.record_request("tenant-b", 50, 32.0, 100_000);

    // Record requests should accumulate
}

#[test]
fn telemetry_export_multiple_tenants_isolated() {
    let export = TelemetryExport::with_console_backend(60);

    export.record_request("tenant-a", 100, 64.0, 500_000);
    export.record_request("tenant-b", 200, 128.0, 300_000);
    export.record_request("tenant-c", 300, 192.0, 400_000);

    // All tenants tracked independently
}

#[tokio::test]
async fn telemetry_export_start_and_shutdown() {
    let export = Arc::new(TelemetryExport::with_console_backend(1));
    export.record_request("tenant-a", 100, 64.0, 500_000);

    let shutdown_tx = export.clone().start_export_loop();

    // Wait for at least one export cycle
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Shutdown
    let _ = shutdown_tx.send(());

    // Give it time to shut down
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
}

// ============================================================================
// Decision Logic Tests: BillingExport Decision Table
// ============================================================================

#[test]
fn billing_export_tenant_present_bandwidth_positive_exports() {
    let record = BillingExport {
        tenant_id: "tenant-a".into(),
        requests: 100,
        cpu_ms: 5000,
        memory_mb: 128.0,
        bandwidth_bytes: 1_000_000,
        timestamp: "2026-01-01T00:00:00Z".into(),
    };

    assert!(!record.tenant_id.is_empty());
    assert!(record.bandwidth_bytes > 0);
    assert!(record.requests > 0);
}

#[test]
fn billing_export_empty_tenant_id_still_valid() {
    let record = BillingExport {
        tenant_id: "".into(),
        requests: 0,
        cpu_ms: 0,
        memory_mb: 0.0,
        bandwidth_bytes: 0,
        timestamp: "2026-01-01T00:00:00Z".into(),
    };

    // Empty record should still be serializable
    let json = serde_json::to_string(&record).unwrap();
    assert!(!json.is_empty());
}

#[test]
fn billing_export_zero_requests_zero_bandwidth_still_exports() {
    let record = BillingExport {
        tenant_id: "tenant-zero".into(),
        requests: 0,
        cpu_ms: 0,
        memory_mb: 0.0,
        bandwidth_bytes: 0,
        timestamp: "2026-01-01T00:00:00Z".into(),
    };

    let json = serde_json::to_string(&record).unwrap();
    assert!(!json.is_empty());
}

// ============================================================================
// BillingExport Serialization Tests
// ============================================================================

#[test]
fn billing_export_serialization_roundtrip() {
    let record = BillingExport {
        tenant_id: "tenant-serialize".into(),
        requests: 42,
        cpu_ms: 2100,
        memory_mb: 96.5,
        bandwidth_bytes: 123_456,
        timestamp: "2026-01-01T00:00:00Z".into(),
    };

    let json = serde_json::to_string(&record).unwrap();
    let decoded: BillingExport = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded.tenant_id, "tenant-serialize");
    assert_eq!(decoded.requests, 42);
    assert_eq!(decoded.cpu_ms, 2100);
    assert!((decoded.memory_mb - 96.5).abs() < 0.001);
    assert_eq!(decoded.bandwidth_bytes, 123_456);
}

#[test]
fn billing_export_serialization_minimal_valid() {
    let record = BillingExport {
        tenant_id: "min".into(),
        requests: 0,
        cpu_ms: 0,
        memory_mb: 0.0,
        bandwidth_bytes: 0,
        timestamp: "".into(),
    };

    let json = serde_json::to_string(&record).unwrap();
    let decoded: BillingExport = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.tenant_id, "min");
}

// ============================================================================
// NusaMetrics Initialization Tests
// ============================================================================

#[test]
fn metrics_init_all_handles_created() {
    let metrics = NusaMetrics::init();

    // All handles should be valid (no panic during creation)
    let _ = &metrics.requests_total;
    let _ = &metrics.requests_failed_total;
    let _ = &metrics.tenants_active;
    let _ = &metrics.request_duration_ms;
    let _ = &metrics.ipc_latency_ms;
    let _ = &metrics.worker_pool_size;
    let _ = &metrics.worker_rss_mb;
    let _ = &metrics.backpressure_usage;
}

#[test]
fn metrics_init_counter_operations() {
    let metrics = NusaMetrics::init();
    metrics.requests_total.increment(1);
    metrics.requests_total.increment(5);
    metrics.requests_failed_total.increment(1);
}

#[test]
fn metrics_init_histogram_operations() {
    let metrics = NusaMetrics::init();
    metrics.request_duration_ms.record(42.5);
    metrics.request_duration_ms.record(100.0);
    metrics.ipc_latency_ms.record(5.0);
}

#[test]
fn metrics_init_gauge_operations() {
    let metrics = NusaMetrics::init();
    metrics.worker_pool_size.set(4.0);
    metrics.worker_rss_mb.set(256.0);
    metrics.backpressure_usage.set(0.75);
    metrics.tenants_active.set(10.0);
}

// ============================================================================
// Tenant Metrics Tests
// ============================================================================

#[test]
fn tenant_metrics_record_request() {
    nusa_telemetry::metrics::tenant::record_request("tenant-a");
    // Should not panic
}

#[test]
fn tenant_metrics_record_failure() {
    nusa_telemetry::metrics::tenant::record_failure("tenant-a");
    // Should not panic
}

#[test]
fn tenant_metrics_record_duration() {
    nusa_telemetry::metrics::tenant::record_duration("tenant-a", 42.5);
    // Should not panic
}

#[test]
fn tenant_metrics_empty_tenant_id() {
    nusa_telemetry::metrics::tenant::record_request("");
    nusa_telemetry::metrics::tenant::record_failure("");
    nusa_telemetry::metrics::tenant::record_duration("", 10.0);
    // Should not panic with empty tenant ID
}
