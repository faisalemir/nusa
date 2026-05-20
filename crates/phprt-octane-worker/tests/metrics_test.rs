//! Integration tests for phprt-octane-worker metrics module.
//!
//! Skills applied:
//! - `m07-concurrency`: Atomic operations for lock-free metrics
//! - `m10-performance`: Running average computation without locks

use phprt_octane_worker::metrics::WorkerMetrics;

// ── WorkerMetrics Basic Operations ──

#[test]
fn worker_metrics_new_starts_at_zero() {
    let metrics = WorkerMetrics::new();
    assert_eq!(metrics.requests_handled(), 0);
    assert_eq!(metrics.error_count(), 0);
    assert_eq!(metrics.error_rate(), 0.0);
    assert_eq!(metrics.avg_response_time_ms(), 0);
}

#[test]
fn worker_metrics_default_same_as_new() {
    let a = WorkerMetrics::new();
    let b = WorkerMetrics::default();
    assert_eq!(a.requests_handled(), b.requests_handled());
}

#[test]
fn worker_metrics_record_request() {
    let metrics = WorkerMetrics::new();
    metrics.record_request(100, true);

    assert_eq!(metrics.requests_handled(), 1);
    assert_eq!(metrics.error_count(), 0);
    assert_eq!(metrics.avg_response_time_ms(), 100);
}

#[test]
fn worker_metrics_record_error() {
    let metrics = WorkerMetrics::new();
    metrics.record_request(100, false);

    assert_eq!(metrics.requests_handled(), 1);
    assert_eq!(metrics.error_count(), 1);
    assert_eq!(metrics.error_rate(), 1.0);
}

#[test]
fn worker_metrics_multiple_requests() {
    let metrics = WorkerMetrics::new();
    metrics.record_request(100, true);
    metrics.record_request(200, true);
    metrics.record_request(300, true);

    assert_eq!(metrics.requests_handled(), 3);
    assert_eq!(metrics.avg_response_time_ms(), 200);
}

#[test]
fn worker_metrics_error_rate_calculation() {
    let metrics = WorkerMetrics::new();
    metrics.record_request(100, true);
    metrics.record_request(100, false);
    metrics.record_request(100, true);
    metrics.record_request(100, false);
    metrics.record_request(100, true);

    assert_eq!(metrics.requests_handled(), 5);
    assert_eq!(metrics.error_count(), 2);
    assert!((metrics.error_rate() - 0.4).abs() < 0.001);
}

#[test]
fn worker_metrics_running_average_stress() {
    let metrics = WorkerMetrics::new();
    for i in 1..=1000 {
        metrics.record_request(i as u64, true);
    }

    // Average of 1..=1000 is (1+1000)/2 = 500.5, truncated to 500
    assert_eq!(metrics.avg_response_time_ms(), 500);
}

#[test]
fn worker_metrics_concurrent_updates() {
    use std::sync::Arc;
    use std::thread;

    let metrics = Arc::new(WorkerMetrics::new());
    let mut handles = vec![];

    for _ in 0..10 {
        let m = Arc::clone(&metrics);
        handles.push(thread::spawn(move || {
            for _ in 0..100 {
                m.record_request(50, true);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(metrics.requests_handled(), 1000);
}
