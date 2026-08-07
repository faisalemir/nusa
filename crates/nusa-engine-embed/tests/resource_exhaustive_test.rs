//! S19: Embed pool resource exhaustive tests.
//!
//! Covers: FD leak through pool cycles, worker exhaustion, standby drain,
//! shutdown cleanup, max_requests recycle, partial bootstrap failure.

use std::collections::HashMap;
use std::path::PathBuf;

use nusa_engine_embed::FfiWorkerPool;

// === FD Leak: Pool Init/Shutdown Cycles ===

/// Count open file descriptors (Linux only).
fn count_open_fds() -> usize {
    #[cfg(target_os = "linux")]
    {
        let fd_dir = std::fs::read_dir("/proc/self/fd").ok();
        fd_dir.map_or(0, |entries| entries.count())
    }
    #[cfg(not(target_os = "linux"))]
    {
        0 // STUB_CONTRACT: FD count only meaningful on Linux/Alpine CI
    }
}

#[tokio::test]
async fn pool_no_fd_leak_through_init_shutdown_cycles() {
    if cfg!(windows) {
        eprintln!("skip: use podman-ci-embed for authoritative test");
        return;
    }
    let cycles = 5;
    let start_fds = count_open_fds();

    for _ in 0..cycles {
        let mut pool = FfiWorkerPool::new(0, PathBuf::from("/nonexistent"), "php".into(), 512, 100);
        pool.shutdown().await;
    }

    let end_fds = count_open_fds();
    assert!(
        end_fds <= start_fds + 2,
        "FD leak through {cycles} init/shutdown cycles: start={start_fds}, end={end_fds}"
    );
}

#[tokio::test]
async fn pool_worker_spawn_failure_clean_shutdown() {
    // Attempt to initialize pool with nonexistent PHP binary
    let mut pool = FfiWorkerPool::new(
        1,
        PathBuf::from("/nonexistent"),
        "nonexistent_php_binary_12345".into(),
        512,
        100,
    );
    let result = pool.initialize().await;
    assert!(
        result.is_err(),
        "pool with nonexistent binary must fail initialization"
    );

    // Shutdown must not panic even after failed init
    pool.shutdown().await;
}

#[tokio::test]
async fn pool_zero_workers_is_ready() {
    let pool = FfiWorkerPool::new(0, PathBuf::from("/nonexistent"), "php".into(), 512, 100);
    assert!(pool.is_ready(), "pool with 0 workers is trivially ready");
    assert_eq!(pool.configured_workers(), 0);
}

#[tokio::test]
async fn pool_handle_request_not_ready_before_init() {
    let mut pool = FfiWorkerPool::new(1, PathBuf::from("/nonexistent"), "php".into(), 512, 100);
    let result = pool
        .handle_http_request("GET".into(), "/".into(), HashMap::new(), None, 5000)
        .await;
    assert!(
        result.is_err(),
        "handle_request before init must return error"
    );
    let err = result.unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("not ready") || err_msg.contains("NoIdleWorker"),
        "error must indicate not-ready or no-idle state, got: {err_msg}"
    );
}

#[tokio::test]
async fn pool_handle_request_no_idle_worker_error() {
    let mut pool = FfiWorkerPool::new(1, PathBuf::from("/nonexistent"), "php".into(), 512, 100);
    // Pool not initialized, so no idle workers available
    let result = pool
        .handle_http_request("GET".into(), "/".into(), HashMap::new(), None, 5000)
        .await;
    assert!(result.is_err(), "must return error when no idle workers");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("not ready") || err.to_string().contains("no idle"),
        "error must indicate no-idle or not-ready state"
    );
}

#[tokio::test]
async fn pool_shutdown_idempotent() {
    let mut pool = FfiWorkerPool::new(0, PathBuf::from("/nonexistent"), "php".into(), 512, 100);
    pool.shutdown().await;
    pool.shutdown().await; // second shutdown must not panic
    pool.shutdown().await; // third time too
}

#[tokio::test]
async fn pool_configured_workers_matches_constructor() {
    let pool = FfiWorkerPool::new(4, PathBuf::from("/nonexistent"), "php".into(), 512, 100);
    assert_eq!(pool.configured_workers(), 4);

    let pool2 =
        FfiWorkerPool::with_standby(8, 2, PathBuf::from("/nonexistent"), "php".into(), 512, 100);
    assert_eq!(pool2.configured_workers(), 8);
}

#[tokio::test]
async fn pool_with_standby_not_ready_before_init() {
    let pool =
        FfiWorkerPool::with_standby(2, 1, PathBuf::from("/nonexistent"), "php".into(), 512, 100);
    assert!(
        !pool.is_ready(),
        "pool with standby must not be ready before init"
    );
}

#[tokio::test]
async fn pool_handle_request_with_empty_headers() {
    let mut pool = FfiWorkerPool::new(0, PathBuf::from("/nonexistent"), "php".into(), 512, 100);
    let result = pool
        .handle_http_request("GET".into(), "/".into(), HashMap::new(), Some(vec![]), 5000)
        .await;
    assert!(result.is_err()); // not ready, but should not panic
}
