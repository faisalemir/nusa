//! Child process engine and process lifecycle management.
//!
//! Skills applied:
//! - `m07-concurrency`: tokio::process for async child management
//! - `m12-lifecycle`: Spawn→communicate→shutdown phases
//! - `m06-error-handling`: IO errors propagate properly

use std::path::Path;
use std::process::Stdio;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tracing::info;

/// Represents a single PHP child process.
///
/// m12-lifecycle: Spawn→communicate→shutdown
/// m07-concurrency: tokio::process for async I/O
pub struct ChildProcess {
    handle: Option<tokio::process::Child>,
    pid: Option<u32>,
}

impl ChildProcess {
    /// Spawn a new PHP child process with piped stdin/stdout/stderr.
    pub async fn spawn(php_binary: &Path, bootstrap_script: &Path) -> std::io::Result<Self> {
        info!(
            "Spawning PHP process: {:?} {:?}",
            php_binary, bootstrap_script
        );

        let mut cmd = Command::new(php_binary);
        if !bootstrap_script.as_os_str().is_empty() {
            cmd.arg(bootstrap_script);
        }
        let child = cmd
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

    /// Write framed IPC data to child's stdin.
    pub async fn write_stdin(&mut self, data: &[u8]) -> std::io::Result<()> {
        if let Some(ref mut child) = self.handle
            && let Some(ref mut stdin) = child.stdin
        {
            stdin.write_all(data).await?;
            stdin.flush().await?;
        }
        Ok(())
    }

    /// Read framed IPC data from child's stdout.
    ///
    /// Reads the 4-byte length prefix, then the payload.
    pub async fn read_stdout(&mut self) -> std::io::Result<Vec<u8>> {
        if let Some(ref mut child) = self.handle
            && let Some(ref mut stdout) = child.stdout
        {
            // Read 4-byte length prefix
            let mut header = [0u8; 4];
            stdout.read_exact(&mut header).await?;
            let len = u32::from_le_bytes(header) as usize;

            // Read payload
            let mut payload = vec![0u8; len];
            stdout.read_exact(&mut payload).await?;

            // Reconstruct framed bytes
            let mut frame = Vec::with_capacity(4 + len);
            frame.extend_from_slice(&header);
            frame.extend_from_slice(&payload);
            return Ok(frame);
        }
        Err(std::io::Error::other("child stdout not available"))
    }

    /// Gracefully terminate the child process.
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
            info!(
                "ChildProcess dropped without explicit shutdown (pid: {:?})",
                self.pid
            );
        }
    }
}
