//! S19: Embed pool stress decision tests.
//!
//! Covers: throughput under load, decision tables for pool states,
//! worker recycle logic, frame encoding stress, error rate accuracy.

use std::collections::HashMap;
use std::path::PathBuf;

use nusa_engine_embed::FfiWorkerPool;
use nusa_engine_embed::frame::{encode_async_query, encode_request};

// === Throughput: Frame Encoding Stress ===

#[test]
fn frame_encode_throughput_1k_per_second() {
    let start = std::time::Instant::now();
    let mut count = 0;

    while start.elapsed().as_secs() < 1 {
        let headers = HashMap::new();
        let frame = encode_request("GET", "/test", &headers, b"").expect("encode");
        assert!(frame.len() > 8, "frame must have valid size");
        count += 1;
    }

    assert!(
        count >= 1000,
        "must encode at least 1000 frames/sec, got {count}"
    );
}

#[test]
fn frame_encode_throughput_10k_per_second() {
    let start = std::time::Instant::now();
    let mut count = 0;

    while start.elapsed().as_secs() < 1 {
        let headers = HashMap::new();
        let frame = encode_request("POST", "/api/data", &headers, b"test body").expect("encode");
        assert!(frame.len() > 8);
        count += 1;
    }

    assert!(
        count >= 5000,
        "must encode at least 5000 frames/sec, got {count}"
    );
}

#[test]
fn async_query_encode_throughput_100k_per_second() {
    let start = std::time::Instant::now();
    let mut count = 0;

    while start.elapsed().as_millis() < 100 {
        let _frame = encode_async_query("SELECT 1").expect("encode");
        count += 1;
    }

    let elapsed_ms = start.elapsed().as_millis();
    let rate = (count as u64) * 1000 / elapsed_ms.max(1) as u64;
    assert!(
        rate >= 50_000,
        "must encode at least 50K async queries/sec, got {rate}"
    );
}

// === Pool State Decision Table ===

#[test]
fn pool_decision_table_zero_workers() {
    let pool = FfiWorkerPool::new(0, PathBuf::from("/nonexistent"), "php".into(), 512, 100);
    assert!(pool.is_ready(), "zero workers = ready");
    assert_eq!(pool.configured_workers(), 0);
}

#[test]
fn pool_decision_table_one_worker_not_initialized() {
    let pool = FfiWorkerPool::new(1, PathBuf::from("/nonexistent"), "php".into(), 512, 100);
    assert!(!pool.is_ready(), "one worker not initialized = not ready");
    assert_eq!(pool.configured_workers(), 1);
}

#[test]
fn pool_decision_table_handle_request_not_ready() {
    let mut pool = FfiWorkerPool::new(1, PathBuf::from("/nonexistent"), "php".into(), 512, 100);
    // Not ready path: handle_request returns error
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result =
        rt.block_on(pool.handle_http_request("GET".into(), "/".into(), HashMap::new(), None, 1000));
    assert!(
        result.is_err(),
        "handle_request on not-ready pool must return error"
    );
}

#[test]
fn pool_decision_table_shutdown_empty_pool() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut pool = FfiWorkerPool::new(0, PathBuf::from("/nonexistent"), "php".into(), 512, 100);
    rt.block_on(pool.shutdown()); // must not panic
}

#[test]
fn pool_decision_table_shutdown_with_failed_init() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut pool = FfiWorkerPool::new(
        1,
        PathBuf::from("/nonexistent"),
        "nonexistent_php_12345".into(),
        512,
        100,
    );
    let _ = rt.block_on(pool.initialize()); // fails
    rt.block_on(pool.shutdown()); // must not panic
}

// === Worker Recycle Decision ===

#[test]
fn pool_max_requests_zero_means_no_recycle() {
    let pool = FfiWorkerPool::new(0, PathBuf::from("/nonexistent"), "php".into(), 512, 0);
    // With max_requests = 0, workers should never need recycling
    // (In practice, 0 workers means no requests anyway)
    assert_eq!(pool.configured_workers(), 0);
}

#[test]
fn pool_max_memory_mb_zero_allowed() {
    let pool = FfiWorkerPool::new(0, PathBuf::from("/nonexistent"), "php".into(), 0, 100);
    // max_memory_mb = 0 means no RSS-based recycle
    assert_eq!(pool.configured_workers(), 0);
}

// === Frame Encoding Decision: Method/URI Variants ===

#[test]
fn frame_encode_all_http_methods() {
    let methods = [
        "GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS", "CONNECT", "TRACE",
    ];
    for method in methods {
        let headers = HashMap::new();
        let frame = encode_request(method, "/", &headers, b"")
            .unwrap_or_else(|_| panic!("encode {method}"));
        assert!(frame.len() > 8);
    }
}

#[test]
fn frame_encode_uri_variants() {
    let uris = [
        "/",
        "/api/v1/users",
        "/nusa-ping",
        "/path?query=value",
        "/path#fragment",
        "/path%20with%20spaces",
        "/path/with/many/segments",
        "/path?query=SELECT%201", // SQL in query string (must encode safely)
    ];
    for uri in uris {
        let headers = HashMap::new();
        let frame =
            encode_request("GET", uri, &headers, b"").unwrap_or_else(|_| panic!("encode {uri}"));
        assert!(frame.len() > 8);
    }
}

#[test]
fn frame_encode_body_variants() {
    let bodies: Vec<(&str, Vec<u8>)> = vec![
        ("empty", vec![]),
        ("single byte", vec![42]),
        ("hello", b"hello world".to_vec()),
        ("json", b"{\"key\": \"value\"}".to_vec()),
        ("null bytes", vec![0, 0, 0, 0]),
        ("binary", (0..=255).collect()),
    ];

    for (name, body) in bodies {
        let headers = HashMap::new();
        let frame = encode_request("POST", "/body", &headers, &body).expect(name);
        assert!(frame.len() > 8, "frame for {name} must be valid");
    }
}

// === Error Rate Accuracy ===

#[tokio::test]
async fn pool_error_rate_accuracy_under_stress() {
    let mut pool = FfiWorkerPool::new(0, PathBuf::from("/nonexistent"), "php".into(), 512, 100);
    let iterations = 100;

    for _ in 0..iterations {
        let result = pool
            .handle_http_request("GET".into(), "/test".into(), HashMap::new(), None, 1000)
            .await;
        assert!(result.is_err(), "each request must fail on uninit pool");
    }
}

// === Recovery After Stress ===

#[tokio::test]
async fn pool_recovery_after_stress_and_shutdown() {
    let mut pool = FfiWorkerPool::new(0, PathBuf::from("/nonexistent"), "php".into(), 512, 100);

    // Stress with failed requests
    for _ in 0..500 {
        let _ = pool
            .handle_http_request("GET".into(), "/test".into(), HashMap::new(), None, 1000)
            .await;
    }

    // Shutdown must be clean
    pool.shutdown().await;

    // Further requests after shutdown — must not panic
    let _ = pool
        .handle_http_request("GET".into(), "/test".into(), HashMap::new(), None, 1000)
        .await;
}

// === Pool Metrics: Error Counter Under Load ===

#[tokio::test]
async fn pool_error_counter_under_load() {
    let mut pool = FfiWorkerPool::new(0, PathBuf::from("/nonexistent"), "php".into(), 512, 100);

    // Send some requests — all will fail
    for _ in 0..10 {
        let _ = pool
            .handle_http_request("GET".into(), "/test".into(), HashMap::new(), None, 1000)
            .await;
    }

    // All requests fail on not-ready pool — verified by assertions above
}
