//! `LaravelHttpRuntime` for the IPC `WorkerPool`.

use std::collections::HashMap;

use async_trait::async_trait;
use nusa_core::{EngineError, LaravelHttpRuntime, PhpResponse, Result};

use crate::error::WorkerError;
use crate::pool::WorkerPool;

fn map_worker_err(e: WorkerError) -> EngineError {
    EngineError::IpcProtocol(e.to_string())
}

#[async_trait]
impl LaravelHttpRuntime for WorkerPool {
    fn is_ready(&self) -> bool {
        WorkerPool::is_ready(self)
    }

    fn configured_workers(&self) -> usize {
        WorkerPool::configured_workers(self)
    }

    async fn handle_http_request(
        &mut self,
        method: String,
        uri: String,
        headers: HashMap<String, Vec<String>>,
        body: Option<Vec<u8>>,
        timeout_ms: u64,
    ) -> Result<PhpResponse> {
        WorkerPool::handle_http_request(self, method, uri, headers, body, timeout_ms)
            .await
            .map_err(map_worker_err)
    }

    async fn recycle_workers(&mut self) -> Result<()> {
        self.shutdown().await.map_err(map_worker_err)?;
        self.initialize().await.map_err(map_worker_err)?;
        Ok(())
    }

    async fn shutdown(&mut self) {
        let _ = WorkerPool::shutdown(self).await;
    }
}
