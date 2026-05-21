//! Child process engine and process lifecycle management.
//!
//! Spawns PHP child processes and communicates via framed IPC over stdin/stdout.
//! Each request is serialized to an `IpcMessage::Request`, sent to the process,
//! and the `IpcMessage::Response` is parsed into a `PhpResponse`.
//!
//! Skills applied:
//! - `m07-concurrency`: tokio::process for async child management
//! - `m12-lifecycle`: Spawn→communicate→shutdown phases
//! - `m06-error-handling`: IO errors propagate properly

use async_trait::async_trait;
use tracing::info;

use nusa_core::{EngineError, PhpEngine, PhpResponse, RequestContext, Result};
use nusa_ipc::protocol::IpcMessage;
use std::time::Duration;

/// PHP engine running as child processes with IPC communication.
///
/// m07-concurrency: tokio::process for async spawning
/// m12-lifecycle: spawn→communicate→kill
/// m06-error-handling: process exit codes -> EngineError
/// domain-cloud-native: OS-level isolation
pub struct ChildEngine {
    php_binary: std::path::PathBuf,
    bootstrap_script: std::path::PathBuf,
}

impl ChildEngine {
    pub fn new(php_binary: std::path::PathBuf, bootstrap_script: std::path::PathBuf) -> Self {
        Self {
            php_binary,
            bootstrap_script,
        }
    }

    pub fn with_default_php() -> Self {
        Self::new(
            std::path::PathBuf::from("php"),
            std::path::PathBuf::from("index.php"),
        )
    }
}

#[async_trait]
impl PhpEngine for ChildEngine {
    async fn execute(&self, ctx: RequestContext) -> Result<PhpResponse> {
        // Calculate timeout from deadline
        let timeout_ms = ctx
            .deadline()
            .saturating_duration_since(tokio::time::Instant::now())
            .as_millis() as u64;

        // Build request context from available data
        // Method and URI are not stored in RequestContext — derive from headers or use defaults
        let method = ctx
            .headers()
            .get("X-Request-Method")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("GET")
            .to_string();
        let uri = ctx
            .headers()
            .get("X-Request-Uri")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("/")
            .to_string();

        let request = IpcMessage::Request {
            id: nusa_ipc::RequestId::new(),
            method,
            uri,
            headers: ctx
                .headers()
                .iter()
                .map(|(k, v)| {
                    (
                        k.as_str().to_string(),
                        vec![v.to_str().unwrap_or("").to_string()],
                    )
                })
                .collect(),
            query: Default::default(),
            post: Default::default(),
            cookies: Default::default(),
            files: vec![],
            body: Some(ctx.body().to_vec()),
            server: ctx.env().iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            timeout_ms,
            trace_context: None,
        };

        // Send request to child process via stdin/stdout IPC
        let framed = request
            .to_framed_bytes()
            .map_err(|e| EngineError::IpcProtocol(format!("serialization failed: {e}")))?;

        let mut child =
            crate::process::ChildProcess::spawn(&self.php_binary, &self.bootstrap_script)
                .await
                .map_err(|e| EngineError::PhpFatal(format!("spawn failed: {e}")))?;

        // Send request via stdin
        child
            .write_stdin(&framed)
            .await
            .map_err(|e| EngineError::IpcProtocol(format!("stdin write failed: {e}")))?;

        // Read response from stdout with timeout
        let response_bytes =
            tokio::time::timeout(Duration::from_millis(timeout_ms), child.read_stdout())
                .await
                .map_err(|_| EngineError::Timeout)?
                .map_err(|e| EngineError::IpcProtocol(format!("stdout read failed: {e}")))?;

        // Parse response
        let response = IpcMessage::from_framed_bytes(&response_bytes)
            .map_err(|e| EngineError::IpcProtocol(format!("deserialization failed: {e}")))?;

        // Wait for child to finish
        let _ = child.shutdown().await;

        match response {
            IpcMessage::Response {
                status,
                headers,
                body,
                ..
            } => {
                let mut header_map = http::HeaderMap::new();
                for (k, values) in headers {
                    for v in values {
                        if let Ok(name) = http::HeaderName::try_from(&k)
                            && let Ok(val) = http::HeaderValue::try_from(&v)
                        {
                            header_map.append(name, val);
                        }
                    }
                }

                Ok(PhpResponse {
                    status,
                    headers: header_map,
                    body: bytes::Bytes::from(body),
                })
            }
            other => Err(EngineError::IpcProtocol(format!(
                "expected Response message, got {:?}",
                std::mem::discriminant(&other)
            ))),
        }
    }

    fn capabilities(&self) -> &'static [&'static str] {
        &["child", "process", "isolated", "ipc"]
    }

    async fn shutdown(&self) {
        info!("Child engine shutting down");
    }
}
