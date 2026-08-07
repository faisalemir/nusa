//! Laravel Octane HTTP runtime abstraction (IPC or embed backends).
//!
//! Gateway dispatches Tier-L traffic through this trait instead of a concrete `WorkerPool`.

use std::collections::HashMap;

use async_trait::async_trait;

use crate::{PhpResponse, Result};

/// Long-lived Laravel worker pool (Octane): IPC subprocesses or embed PHP.
#[async_trait]
pub trait LaravelHttpRuntime: Send + Sync + 'static {
    /// Pool configured and every worker can accept traffic.
    fn is_ready(&self) -> bool;

    /// Configured worker count (`octane_workers`).
    fn configured_workers(&self) -> usize;

    /// Dispatch one HTTP request through an idle worker.
    async fn handle_http_request(
        &mut self,
        method: String,
        uri: String,
        headers: HashMap<String, Vec<String>>,
        body: Option<Vec<u8>>,
        timeout_ms: u64,
    ) -> Result<PhpResponse>;

    /// Recycle workers after config/code changes (dev hot-reload).
    async fn recycle_workers(&mut self) -> Result<()>;

    /// Shut down all workers.
    async fn shutdown(&mut self);
}
