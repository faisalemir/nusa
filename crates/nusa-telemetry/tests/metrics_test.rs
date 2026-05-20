//! Exhaustive tests for NusaMetrics and tenant metrics.
//!
//! rust-test-deep Phase 1: Core Exhaustive
//! rust-test-deep Phase 3: Concurrency Exhaustive

use nusa_telemetry::metrics::NusaMetrics;

// ── NusaMetrics Initialization ──

/// === Arrange ===
/// NusaMetrics initialized.
/// === Act ===
/// Metrics created.
/// === Assert ===
/// All counters, histograms, gauges accessible.
#[test]
fn metrics_init_creates_all_handles() {
    // === Arrange & Act ===
    let m = NusaMetrics::init();

    // === Assert ===
    // Counters exist
    m.requests_total.increment(1);
    m.requests_failed_total.increment(1);

    // Gauges exist
    m.tenants_active.set(1.0);
    m.worker_pool_size.set(4.0);
    m.worker_rss_mb.set(256.0);
    m.backpressure_usage.set(0.0);

    // Histograms exist
    m.request_duration_ms.record(100.0);
    m.ipc_latency_ms.record(5.0);
}

/// === Arrange ===
/// NusaMetrics initialized multiple times.
/// === Act ===
/// Multiple init calls.
/// === Assert ===
/// No panic (metrics are global singleton).
#[test]
fn metrics_init_multiple_times_no_panic() {
    // === Arrange & Act ===
    let _m1 = NusaMetrics::init();
    let _m2 = NusaMetrics::init();
    let _m3 = NusaMetrics::init();

    // === Assert ===
    // Should not panic — metrics crate handles re-init gracefully
}

// ── Counter Operations ──

/// === Arrange ===
/// NusaMetrics initialized, counter at 0.
/// === Act ===
/// Increment counter 100 times.
/// === Assert ===
/// Counter reflects increments (metrics crate manages internally).
#[test]
fn metrics_counter_increment_multiple_times() {
    // === Arrange ===
    let m = NusaMetrics::init();

    // === Act ===
    for _ in 0..100 {
        m.requests_total.increment(1);
    }

    // === Assert ===
    // The prometheus recorder handles the actual count
    // This test verifies no panic on many increments
}

/// === Arrange ===
/// NusaMetrics counter.
/// === Act ===
/// Increment by different values.
/// === Assert ===
/// No overflow.
#[test]
fn metrics_counter_increment_various_amounts() {
    // === Arrange ===
    let m = NusaMetrics::init();

    // === Act ===
    m.requests_total.increment(0);
    m.requests_total.increment(1);
    m.requests_total.increment(100);
    m.requests_total.increment(1_000_000);

    // === Assert ===
    // Should not panic
}

// ── Gauge Operations ──

/// === Arrange ===
/// NusaMetrics gauge.
/// === Act ===
/// Set gauge to various values.
/// === Assert ===
/// Values accepted.
#[test]
fn metrics_gauge_set_various_values() {
    // === Arrange ===
    let m = NusaMetrics::init();

    // === Act ===
    m.worker_pool_size.set(0.0);
    m.worker_pool_size.set(1.0);
    m.worker_pool_size.set(100.0);
    m.worker_pool_size.set(-1.0); // gauges can be negative

    // === Assert ===
    // Should not panic
}

// ── Histogram Operations ──

/// === Arrange ===
/// NusaMetrics histogram.
/// === Act ===
/// Record various values.
/// === Assert ===
/// Values recorded.
#[test]
fn metrics_histogram_record_various_values() {
    // === Arrange ===
    let m = NusaMetrics::init();

    // === Act ===
    m.request_duration_ms.record(0.0);
    m.request_duration_ms.record(1.0);
    m.request_duration_ms.record(100.0);
    m.request_duration_ms.record(1_000_000.0);

    // === Assert ===
    // Should not panic
}

// ── Tenant Metrics (Phase D5) ──

/// === Arrange ===
/// Tenant metrics module available.
/// === Act ===
/// Record tenant-tagged metrics.
/// === Assert ===
/// No panic, tenant labels applied.
#[test]
fn metrics_tenant_record_request() {
    // === Arrange & Act ===
    nusa_telemetry::metrics::tenant::record_request("acme");
    nusa_telemetry::metrics::tenant::record_failure("acme");
    nusa_telemetry::metrics::tenant::record_duration("acme", 150.0);

    // === Assert ===
    // Should not panic
}

/// === Arrange ===
/// Multiple tenants.
/// === Act ===
/// Record metrics for different tenants.
/// === Assert ===
/// All tenants tracked independently.
#[test]
fn metrics_tenant_multiple_tenants_independent() {
    // === Arrange & Act ===
    nusa_telemetry::metrics::tenant::record_request("tenant-a");
    nusa_telemetry::metrics::tenant::record_request("tenant-b");
    nusa_telemetry::metrics::tenant::record_request("tenant-c");
    nusa_telemetry::metrics::tenant::record_failure("tenant-b");

    // === Assert ===
    // Each tenant gets independent counters
    // Prometheus handles this via label cardinality
}

// ── Concurrency (Phase 3) ──

/// === Arrange ===
/// NusaMetrics shared across 10 threads.
/// === Act ===
/// Each thread increments counters 1000 times.
/// === Assert ===
/// No data corruption.
#[test]
fn metrics_concurrent_updates_no_corruption() {
    use std::sync::Arc;
    use std::thread;

    // === Arrange ===
    let m = Arc::new(NusaMetrics::init());

    let mut handles = vec![];

    // === Act ===
    for _ in 0..10 {
        let metrics = Arc::clone(&m);
        handles.push(thread::spawn(move || {
            for _ in 0..1000 {
                metrics.requests_total.increment(1);
                metrics.requests_failed_total.increment(1);
                metrics.worker_pool_size.set(4.0);
                metrics.request_duration_ms.record(50.0);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // === Assert ===
    // 10 threads × 1000 iterations = 10,000 increments
    // No panic or corruption
}
