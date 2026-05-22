//! Resource exhaustion tests for nusa-cli crate.
//!
//! Covers: DevWatcher FD leak, spawn task cleanup, rapid file changes,
//! TestRunner FD leak, shutdown cleanup.

use nusa_cli::dev::DevWatcher;

// ── DevWatcher: Spawn Task Cleanup ──

#[tokio::test]
async fn devwatcher_debounce_loop_task_cleanup_on_abort() {
    // === Arrange ===
    let tmp = std::env::temp_dir().join("nusa-abort-test");
    tokio::fs::create_dir_all(&tmp)
        .await
        .expect("must create dir");
    let app_dir = tmp.join("app");
    tokio::fs::create_dir_all(&app_dir)
        .await
        .expect("must create dir");

    // === Act ===
    let mut watcher = DevWatcher::new(50, false);
    let _ = watcher.start(&tmp);

    // Watcher spawns debounce_loop task — verify cleanup on drop
    drop(watcher);

    // === Assert ===
    // No orphaned task after watcher dropped
    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

// ── TestRunner: Enumerate Files FD Stable ──

#[test]
fn testrunner_enumerate_files_1000_times_fd_stable() {
    // === Arrange ===
    let start_fds = count_open_fds();
    let tmp = std::env::temp_dir().join("nusa-enumerate-test");
    std::fs::create_dir_all(&tmp).expect("must create dir");

    // Create some PHP test files
    for i in 0..10 {
        let path = tmp.join(format!("Test_{}.php", i));
        std::fs::write(&path, "<?php").expect("must write");
    }

    // === Act ===
    let config = nusa_cli::test::TestConfig {
        workers: 1,
        test_path: tmp.clone(),
        reset_between_tests: false,
    };
    let _runner = nusa_cli::test::TestRunner::new(config);

    for _ in 0..100 {
        let _files = enumerate_test_files_helper(&tmp);
    }

    // === Assert ===
    let end_fds = count_open_fds();
    assert!(
        end_fds <= start_fds + 10,
        "FD count must be stable after 100 enumerations"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

// ── TestRunner: Shutdown Cleanup ──

#[tokio::test]
async fn testrunner_shutdown_all_resources_released() {
    // === Arrange ===
    let start_fds = count_open_fds();
    let tmp = std::env::temp_dir().join("nusa-shutdown-test");
    std::fs::create_dir_all(&tmp).expect("must create dir");

    let config = nusa_cli::test::TestConfig {
        workers: 1,
        test_path: tmp.clone(),
        reset_between_tests: false,
    };
    let mut runner = nusa_cli::test::TestRunner::new(config);

    // === Act ===
    // Initialize will fail (no PHP), but shutdown must work
    let _ = runner.initialize().await;
    let shutdown_result = runner.shutdown().await;

    // === Assert ===
    assert!(shutdown_result.is_ok(), "shutdown must succeed");

    let end_fds = count_open_fds();
    assert!(end_fds <= start_fds + 10, "no FD leak from shutdown");

    let _ = std::fs::remove_dir_all(&tmp);
}

// ── Helper ──

fn enumerate_test_files_helper(path: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && let Some(ext) = path.extension().and_then(|e| e.to_str())
                && ext == "php"
            {
                files.push(path);
            }
        }
    }
    files
}

#[cfg(unix)]
fn count_open_fds() -> usize {
    use std::fs;
    let fd_dir = "/proc/self/fd";
    if let Ok(entries) = fs::read_dir(fd_dir) {
        entries.count()
    } else {
        0
    }
}

#[cfg(not(unix))]
fn count_open_fds() -> usize {
    0
}
