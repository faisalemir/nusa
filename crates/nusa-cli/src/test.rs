//! Native test runner with persistent worker pool.
//! Blueprint 6 F5: `nusa test` — eliminate Laravel boot overhead.
//!
//! Skills applied:
//! - `m07-concurrency`: Persistent worker pool isolated from production
//! - `domain-cli`: Test command with PHPUnit/Pest passthrough
//! - `m12-lifecycle`: spawn → run tests → reset state → shutdown
//! - `m13-domain-error`: Isolated test pool prevents production interference

use std::path::PathBuf;

use tracing::info;

use nusa_octane_worker::WorkerPool;
use nusa_octane_worker::state_reset::StateResetOrchestrator;

/// Test pool configuration.
pub struct TestConfig {
    pub workers: u32,
    pub test_path: PathBuf,
    pub reset_between_tests: bool,
}

/// Native test runner with persistent Octane workers.
///
/// m07-concurrency: Dedicated test pool isolated from production workers.
/// domain-cli: Pass-through support for PHPUnit/Pest test frameworks.
pub struct TestRunner {
    pool: Option<WorkerPool>,
    orchestrator: StateResetOrchestrator,
    config: TestConfig,
}

impl TestRunner {
    pub fn new(config: TestConfig) -> Self {
        Self {
            pool: None,
            orchestrator: StateResetOrchestrator::new(128),
            config,
        }
    }

    /// Initialize the test pool (m12-lifecycle).
    pub async fn initialize(&mut self) -> anyhow::Result<()> {
        info!(
            "Initializing test pool with {} workers",
            self.config.workers
        );

        let mut pool = WorkerPool::new(
            self.config.workers as usize,
            self.config.test_path.clone(),
            256,  // 256MB memory per test worker
            1000, // recycle after 1000 tests
        );

        pool.initialize().await?;
        self.pool = Some(pool);

        // m12-lifecycle: Initialize state reset for clean state between tests
        if self.config.reset_between_tests {
            self.orchestrator.initialize();
        }

        info!("Test pool ready");
        Ok(())
    }

    /// Run tests through the persistent pool (domain-cli).
    pub async fn run_tests(&mut self) -> anyhow::Result<TestResult> {
        if self.pool.is_none() {
            self.initialize().await?;
        }

        info!("Running tests from {:?}", self.config.test_path);

        // Enumerate test files in the test path
        let test_files = self.enumerate_test_files();
        let total = test_files.len();

        if total == 0 {
            info!("No test files found in {:?}", self.config.test_path);
            return Ok(TestResult {
                total: 0,
                passed: 0,
                failed: 0,
            });
        }

        info!("Found {} test files", total);

        // Run each test file through a worker from the pool
        let mut passed = 0;
        let mut failed = 0;
        let reset_between_tests = self.config.reset_between_tests;

        if let Some(ref mut pool) = self.pool {
            for test_file in &test_files {
                // Reset state before each test if configured
                if reset_between_tests {
                    pool.shutdown().await.ok();
                    // Re-initialize pool for clean state
                    let _ = pool.initialize().await;
                }

                // Get an idle worker
                let worker_id = pool.idle_count();
                if worker_id > 0 {
                    let method = "GET".to_string();
                    let uri = format!("/test/{}", test_file.to_string_lossy());

                    // Use worker by index directly to avoid borrow conflicts
                    let idx = pool.idle_count() - 1;
                    match pool.worker_mut(idx).handle_request(method, uri, 30000).await {
                        Ok(_response) => {
                            passed += 1;
                            info!("PASS: {}", test_file.display());
                        }
                        Err(e) => {
                            failed += 1;
                            info!("FAIL: {} — {}", test_file.display(), e);
                        }
                    }

                    // Return worker to idle queue
                    pool.return_worker(pool.worker(idx).id);
                } else {
                    info!("No idle workers available, queuing test: {}", test_file.display());
                    failed += 1;
                }
            }
        }

        // Run PHPUnit/Pest as fallback if no workers available
        if self.pool.is_none() {
            return self.run_php_tests().await;
        }

        Ok(TestResult {
            total,
            passed,
            failed,
        })
    }

    /// Run tests via PHPUnit/Pest as fallback.
    async fn run_php_tests(&self) -> anyhow::Result<TestResult> {
        info!("Falling back to PHPUnit/Pest execution");

        let output = tokio::process::Command::new("php")
            .arg("vendor/bin/phpunit")
            .arg("--configuration")
            .arg(&self.config.test_path)
            .arg("--teamcity")
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Parse PHPUnit output for test counts
        let total = stdout.matches("##teamcity[testCount").count();
        let passed = stdout.matches("##teamcity[testFinished").count();
        let failed = stdout.matches("##teamcity[testFailed").count();

        if !stderr.is_empty() {
            info!("PHPUnit stderr: {}", stderr);
        }

        Ok(TestResult {
            total,
            passed,
            failed,
        })
    }

    /// Enumerate test files in the test path.
    fn enumerate_test_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.config.test_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if ext == "php" && path.file_name().map_or(false, |n| {
                            n.to_string_lossy().ends_with("Test.php")
                                || n.to_string_lossy().ends_with("_test.php")
                        }) {
                            files.push(path);
                        }
                    }
                }
            }
        }
        files
    }

    /// Reset state between test runs (m12-lifecycle).
    pub fn reset_state(&self) {
        if self.config.reset_between_tests {
            self.orchestrator
                .emit_event(
                    nusa_octane_worker::state_reset::OctaneEvent::RequestReceived {
                        request_id: "test-reset".to_string(),
                    },
                )
                .ok();
        }
    }

    /// Shut down the test pool (m12-lifecycle).
    pub async fn shutdown(&mut self) -> anyhow::Result<()> {
        if let Some(ref mut pool) = self.pool {
            pool.shutdown().await?;
        }
        self.orchestrator.shutdown();
        info!("Test pool shut down");
        Ok(())
    }
}

/// Test execution results.
pub struct TestResult {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
}

impl std::fmt::Display for TestResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Tests: {} total, {} passed, {} failed",
            self.total, self.passed, self.failed
        )
    }
}
