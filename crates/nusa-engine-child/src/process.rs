//! Child process engine and process lifecycle management.
//!
//! Skills applied:
//! - `m07-concurrency`: tokio::process for async child management
//! - `m12-lifecycle`: Spawn→communicate→shutdown phases
//! - `m06-error-handling`: IO errors propagate properly

use std::path::Path;
use std::process::Stdio;

use tokio::process::Command;
use tracing::info;

// Represents a single PHP child process.
//
// m12-lifecycle: Spawn→communicate→shutdown
// m07-concurrency: tokio::process for async I/O
pub struct ChildProcess {
    handle: Option<tokio::process::Child>,
    pid: Option<u32>,
}

impl ChildProcess {
    // Spawn a new PHP child process.
    pub async fn spawn(
        php_binary: &Path,
        bootstrap_script: &Path,
    ) -> std::io::Result<Self> {
        info!(
            "Spawning PHP process: {:?} {:?}",
            php_binary, bootstrap_script
        );

        let child = Command::new(php_binary)
            .arg(bootstrap_script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let pid = child.id();
        Ok(Self {
            handle: Some(child),
            pid,
        })
    }

    #[must_use]
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    // Gracefully terminate the child process.
    pub async fn shutdown(&mut self) -> std::io::Result<()> {
        if let Some(ref mut child) = self.handle {
            info!("Shutting down PHP process (pid: {:?})", self.pid);
            child.kill().await?;
            child.wait().await?;
        }
        self.handle = None;
        Ok(())
    }
}

impl Drop for ChildProcess {
    fn drop(&mut self) {
        if self.handle.is_some() {
            info!("ChildProcess dropped without explicit shutdown (pid: {:?})", self.pid);
        }
    }
}
