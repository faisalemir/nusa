use async_trait::async_trait;
use tracing::info;

use phprt_core::{PhpEngine, RequestContext, PhpResponse, Result};

/// PHP engine running as child processes.
///
/// m07-concurrency: tokio::process for async spawning
/// m12-lifecycle: spawn→communicate→kill
/// m06-error-handling: process exit codes -> EngineError
/// domain-cloud-native: OS-level isolation
pub struct ChildEngine {
    #[allow(dead_code)]
    php_binary: std::path::PathBuf,
}

impl ChildEngine {
    pub fn new(php_binary: std::path::PathBuf) -> Self {
        Self {
            php_binary,
        }
    }

    pub fn with_default_php() -> Self {
        Self::new(std::path::PathBuf::from("php"))
    }
}

#[async_trait]
impl PhpEngine for ChildEngine {
    async fn execute(&self, _ctx: RequestContext) -> Result<PhpResponse> {

        // TODO: Spawn child process
        // TODO: Send request via stdin (framed IPC)
        // TODO: Read response from stdout
        // TODO: Parse HTTP status, headers, body
        // TODO: Handle non-zero exit code -> PhpFatal

        // Stub
        Ok(PhpResponse {
            status: 200,
            headers: http::HeaderMap::new(),
            body: bytes::Bytes::from("Child engine stub - not yet implemented"),
        })
    }

    fn capabilities(&self) -> &'static [&'static str] {
        &["child", "process", "isolated"]
    }

    async fn shutdown(&self) {
        info!("Child engine shutting down");
        // TODO: Signal all worker processes to stop
        // TODO: Wait for graceful exit or force-kill after timeout
    }

}
