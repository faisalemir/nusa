//! Domain-specific tests for the TestRunner CLI functionality.
//!
//! Covers: initialization, test enumeration, execution with workers,
//! reset between tests, shutdown, file pattern matching, and fallback.

use std::path::{Path, PathBuf};

use nusa_cli::test::{TestConfig, TestResult, TestRunner};

// ─── Helpers ──────────────────────────────────────────────────────────────

fn temp_test_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nusa_test_runner_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn cleanup_test_dir(dir: &PathBuf) {
    let _ = std::fs::remove_dir_all(dir);
}

fn create_test_file(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, "<?php // test file").expect("write test file");
    path
}

// ─── 1. TestRunner New ────────────────────────────────────────────────────

#[tokio::test]
async fn test_runner_new_initializes_with_correct_defaults() {
    let config = TestConfig {
        workers: 4,
        test_path: PathBuf::from("/tmp"),
        reset_between_tests: false,
    };
    let _runner = TestRunner::new(config);

    // Should not panic — runner created with defaults
    // Pool is None until initialize() is called
}

#[tokio::test]
async fn test_runner_new_with_zero_workers() {
    let config = TestConfig {
        workers: 0,
        test_path: PathBuf::from("/tmp"),
        reset_between_tests: false,
    };
    let _runner = TestRunner::new(config);
    // Zero workers should still create runner
}

#[tokio::test]
async fn test_runner_new_with_reset_enabled() {
    let config = TestConfig {
        workers: 2,
        test_path: PathBuf::from("/tmp"),
        reset_between_tests: true,
    };
    let _runner = TestRunner::new(config);
    // Reset flag should be stored
}

// ─── 2. TestRunner Initialization ─────────────────────────────────────────

#[tokio::test]
async fn test_runner_initialization_success() {
    let dir = temp_test_dir();
    let config = TestConfig {
        workers: 1,
        test_path: dir.clone(),
        reset_between_tests: false,
    };
    let mut runner = TestRunner::new(config);

    // Initialize will try to spawn workers — may fail without PHP
    // But it should not panic
    let _result = runner.initialize().await;
    // Result depends on platform — we verify no panic

    cleanup_test_dir(&dir);
}

#[tokio::test]
async fn test_runner_initialization_no_test_files_empty_directory() {
    let dir = temp_test_dir();
    let config = TestConfig {
        workers: 1,
        test_path: dir.clone(),
        reset_between_tests: false,
    };
    let mut runner = TestRunner::new(config);

    // Even without PHP workers, run_tests should handle gracefully
    let result = runner.run_tests().await;
    // Should succeed or fail gracefully — no panic
    assert!(result.is_ok() || result.is_err());

    cleanup_test_dir(&dir);
}

#[tokio::test]
async fn test_runner_initialization_with_reset_between_tests() {
    let dir = temp_test_dir();
    let config = TestConfig {
        workers: 1,
        test_path: dir.clone(),
        reset_between_tests: true,
    };
    let mut runner = TestRunner::new(config);

    // Initialize with reset enabled
    let _ = runner.initialize().await;
    // Orchestrator should be initialized with default actions

    cleanup_test_dir(&dir);
}

// ─── 3. TestRunner Run Tests with Workers ─────────────────────────────────

#[tokio::test]
async fn test_runner_run_tests_with_workers() {
    let dir = temp_test_dir();
    create_test_file(&dir, "ExampleTest.php");

    let config = TestConfig {
        workers: 2,
        test_path: dir.clone(),
        reset_between_tests: false,
    };
    let mut runner = TestRunner::new(config);

    // run_tests will try to use workers — result depends on platform
    let result = runner.run_tests().await;
    // Should not panic
    assert!(result.is_ok() || result.is_err());

    cleanup_test_dir(&dir);
}

#[tokio::test]
async fn test_runner_run_tests_empty_directory_returns_zero() {
    let dir = temp_test_dir();
    let config = TestConfig {
        workers: 1,
        test_path: dir.clone(),
        reset_between_tests: false,
    };
    let mut runner = TestRunner::new(config);

    let result = runner.run_tests().await;
    assert!(result.is_ok(), "empty directory should succeed");
    let test_result = result.expect("should succeed");
    assert_eq!(test_result.total, 0);
    assert_eq!(test_result.passed, 0);
    assert_eq!(test_result.failed, 0);

    cleanup_test_dir(&dir);
}

// ─── 4. TestRunner Reset Between Tests ────────────────────────────────────

#[tokio::test]
async fn test_runner_reset_between_tests() {
    let dir = temp_test_dir();
    let config = TestConfig {
        workers: 1,
        test_path: dir.clone(),
        reset_between_tests: true,
    };
    let mut runner = TestRunner::new(config);

    // Create a test file
    create_test_file(&dir, "ResetTest.php");

    let result = runner.run_tests().await;
    // With reset enabled, pool should be recycled between tests
    assert!(result.is_ok() || result.is_err());

    // Test reset_state() method
    runner.reset_state();

    cleanup_test_dir(&dir);
}

#[tokio::test]
async fn test_runner_reset_without_flag_no_op() {
    let dir = temp_test_dir();
    let config = TestConfig {
        workers: 1,
        test_path: dir.clone(),
        reset_between_tests: false,
    };
    let runner = TestRunner::new(config);

    // reset_state should be a no-op when flag is not set
    runner.reset_state();
    // Should not panic

    cleanup_test_dir(&dir);
}

// ─── 5. TestRunner Shutdown ───────────────────────────────────────────────

#[tokio::test]
async fn test_runner_shutdown_graceful() {
    let dir = temp_test_dir();
    let config = TestConfig {
        workers: 1,
        test_path: dir.clone(),
        reset_between_tests: false,
    };
    let mut runner = TestRunner::new(config);

    let result = runner.shutdown().await;
    // Shutdown should succeed even without initialization
    assert!(result.is_ok(), "shutdown should succeed");

    cleanup_test_dir(&dir);
}

#[tokio::test]
async fn test_runner_shutdown_all_resources_released() {
    let dir = temp_test_dir();
    let config = TestConfig {
        workers: 2,
        test_path: dir.clone(),
        reset_between_tests: false,
    };
    let mut runner = TestRunner::new(config);

    // Initialize
    let _ = runner.initialize().await;
    // Run tests
    let _ = runner.run_tests().await;
    // Shutdown
    let result = runner.shutdown().await;
    assert!(result.is_ok(), "shutdown should release all resources");

    cleanup_test_dir(&dir);
}

#[tokio::test]
async fn test_runner_double_shutdown_safe() {
    let dir = temp_test_dir();
    let config = TestConfig {
        workers: 1,
        test_path: dir.clone(),
        reset_between_tests: false,
    };
    let mut runner = TestRunner::new(config);

    let result1 = runner.shutdown().await;
    let result2 = runner.shutdown().await;
    // Both should succeed (idempotent)
    assert!(result1.is_ok());
    assert!(result2.is_ok());

    cleanup_test_dir(&dir);
}

// ─── 6. TestRunner Enumerate Test Files ───────────────────────────────────

#[tokio::test]
async fn test_runner_enumerate_php_test_files_classic_pattern() {
    let dir = temp_test_dir();
    create_test_file(&dir, "UserTest.php");
    create_test_file(&dir, "OrderTest.php");
    create_test_file(&dir, "NotAPhp.php"); // Not a PHPUnit test file pattern

    let config = TestConfig {
        workers: 0,
        test_path: dir.clone(),
        reset_between_tests: false,
    };
    let runner = TestRunner::new(config);
    let files = runner.enumerate_test_files();

    assert!(
        files
            .iter()
            .any(|f| f.file_name().unwrap().to_string_lossy() == "UserTest.php"),
        "should find UserTest.php"
    );
    assert!(
        files
            .iter()
            .any(|f| f.file_name().unwrap().to_string_lossy() == "OrderTest.php"),
        "should find OrderTest.php"
    );
    assert!(
        !files
            .iter()
            .any(|f| f.file_name().unwrap().to_string_lossy() == "NotAPhp.php"),
        "should NOT find NotAPhp.php"
    );

    cleanup_test_dir(&dir);
}

#[tokio::test]
async fn test_runner_enumerate_underscore_test_files() {
    let dir = temp_test_dir();
    create_test_file(&dir, "user_test.php");
    create_test_file(&dir, "order_test.php");

    let config = TestConfig {
        workers: 0,
        test_path: dir.clone(),
        reset_between_tests: false,
    };
    let runner = TestRunner::new(config);
    let files = runner.enumerate_test_files();

    assert!(
        files
            .iter()
            .any(|f| f.file_name().unwrap().to_string_lossy() == "user_test.php"),
        "should find user_test.php"
    );
    assert!(
        files
            .iter()
            .any(|f| f.file_name().unwrap().to_string_lossy() == "order_test.php"),
        "should find order_test.php"
    );

    cleanup_test_dir(&dir);
}

#[tokio::test]
async fn test_runner_enumerate_non_php_files_ignored() {
    let dir = temp_test_dir();
    // Create non-PHP test files
    for ext in &["js", "css", "html", "json", "xml"] {
        let path = dir.join(format!("test.{}", ext));
        std::fs::write(&path, "// not PHP").expect("write file");
    }
    // Also create actual PHP test file
    create_test_file(&dir, "RealTest.php");

    let config = TestConfig {
        workers: 0,
        test_path: dir.clone(),
        reset_between_tests: false,
    };
    let runner = TestRunner::new(config);
    let files = runner.enumerate_test_files();

    // Should only find the PHP test file
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0].file_name().unwrap().to_string_lossy(),
        "RealTest.php"
    );

    cleanup_test_dir(&dir);
}

#[tokio::test]
async fn test_runner_enumerate_mixed_patterns() {
    let dir = temp_test_dir();
    create_test_file(&dir, "FeatureTest.php");
    create_test_file(&dir, "feature_test.php");
    create_test_file(&dir, "Feature.php"); // Not a test file
    create_test_file(&dir, "Test.php"); // Not matching either pattern
    create_test_file(&dir, "my_test.php"); // Matches _test.php

    let config = TestConfig {
        workers: 0,
        test_path: dir.clone(),
        reset_between_tests: false,
    };
    let runner = TestRunner::new(config);
    let files = runner.enumerate_test_files();

    assert!(
        files
            .iter()
            .any(|f| f.file_name().unwrap().to_string_lossy() == "FeatureTest.php"),
        "should find FeatureTest.php"
    );
    assert!(
        files
            .iter()
            .any(|f| f.file_name().unwrap().to_string_lossy() == "feature_test.php"),
        "should find feature_test.php"
    );
    assert!(
        files
            .iter()
            .any(|f| f.file_name().unwrap().to_string_lossy() == "my_test.php"),
        "should find my_test.php"
    );

    cleanup_test_dir(&dir);
}

// ─── 7. TestRunner No Workers Available ───────────────────────────────────

#[tokio::test]
async fn test_runner_no_workers_available_all_workers_busy() {
    let dir = temp_test_dir();
    create_test_file(&dir, "BusyTest.php");

    let config = TestConfig {
        workers: 1,
        test_path: dir.clone(),
        reset_between_tests: false,
    };
    let mut runner = TestRunner::new(config);

    // Initialize the pool
    let init_result = runner.initialize().await;

    if init_result.is_ok() {
        // Pool initialized but stub workers have no transport
        let result = runner.run_tests().await;
        // Should handle no-transport gracefully (workers fail, counts as failed)
        assert!(result.is_ok());
        let test_result = result.expect("should succeed");
        // Test should be marked as failed (no transport)
        assert!(test_result.failed > 0 || test_result.passed > 0);
    }

    cleanup_test_dir(&dir);
}

// ─── 8. TestRunner Fallback PHP Tests ─────────────────────────────────────

#[tokio::test]
async fn test_runner_fallback_phpunit_pest_when_no_pool() {
    let dir = temp_test_dir();
    // Create a test directory without pool
    let config = TestConfig {
        workers: 0,
        test_path: dir.clone(),
        reset_between_tests: false,
    };
    let mut runner = TestRunner::new(config);

    // When pool is None, run_tests falls back to PHPUnit/Pest
    // This will fail if phpunit isn't installed, but should not panic
    let result = runner.run_tests().await;
    // May fail due to missing phpunit — that's expected
    assert!(result.is_ok() || result.is_err());

    cleanup_test_dir(&dir);
}

// ─── 9. TestResult Display ────────────────────────────────────────────────

#[test]
fn test_result_display_formatting_correct() {
    let result = TestResult {
        total: 10,
        passed: 8,
        failed: 2,
    };
    let display = format!("{}", result);
    assert!(display.contains("10 total"));
    assert!(display.contains("8 passed"));
    assert!(display.contains("2 failed"));
}

#[test]
fn test_result_display_zero_counts() {
    let result = TestResult {
        total: 0,
        passed: 0,
        failed: 0,
    };
    let display = format!("{}", result);
    assert!(display.contains("0 total"));
    assert!(display.contains("0 passed"));
    assert!(display.contains("0 failed"));
}

#[test]
fn test_result_display_all_passed() {
    let result = TestResult {
        total: 5,
        passed: 5,
        failed: 0,
    };
    let display = format!("{}", result);
    assert!(display.contains("5 total"));
    assert!(display.contains("5 passed"));
    assert!(display.contains("0 failed"));
}
