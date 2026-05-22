//! Octane leak / isolation suite (P2): many sequential requests must not cross-contaminate responses.
//!
//! STUB_CONTRACT: Runs only on Unix with `NUSA_LARAVEL_FIXTURE` prepared (Alpine `just podman-test-laravel`).
//! Set `NUSA_LEAK_REQUESTS` to override count (default 10_000 in CI profile).

#[cfg(unix)]
use nusa_e2e_tests::require_laravel_fixture;
#[cfg(unix)]
use nusa_octane_worker::pool::WorkerPool;
#[cfg(unix)]
use std::collections::HashMap;

#[tokio::test]
#[cfg(unix)]
async fn octane_leak_suite_sequential_requests_stable_body() {
    let root = require_laravel_fixture();
    let iterations: usize = std::env::var("NUSA_LEAK_REQUESTS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000);

    let mut pool = WorkerPool::new(1, root, 512, iterations as u64 + 100);
    pool.initialize()
        .await
        .expect("pool must initialize in Alpine CI");
    assert!(pool.is_ready());

    for i in 0..iterations {
        let response = pool
            .handle_http_request("GET".into(), "/".into(), HashMap::new(), None, 30_000)
            .await
            .unwrap_or_else(|e| panic!("request {i} failed: {e}"));

        assert_eq!(response.status, 200, "request {i} status");
        let body = String::from_utf8_lossy(&response.body);
        assert!(
            body.contains("nusa-fixture-ok"),
            "request {i}: unexpected body (possible state leak): {}",
            &body[..body.len().min(120)]
        );
    }

    pool.shutdown().await.ok();
}
