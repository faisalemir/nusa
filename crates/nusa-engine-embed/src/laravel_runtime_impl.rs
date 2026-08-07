//! `LaravelHttpRuntime` for `FfiWorkerPool`.

use std::collections::HashMap;

use async_trait::async_trait;
use nusa_core::{EngineError, LaravelHttpRuntime, PhpResponse, Result};

use crate::error::EmbedError;
use crate::pool::FfiWorkerPool;

fn map_embed_err(e: EmbedError) -> EngineError {
    EngineError::IpcProtocol(e.to_string())
}

#[async_trait]
impl LaravelHttpRuntime for FfiWorkerPool {
    fn is_ready(&self) -> bool {
        FfiWorkerPool::is_ready(self)
    }

    fn configured_workers(&self) -> usize {
        FfiWorkerPool::configured_workers(self)
    }

    async fn handle_http_request(
        &mut self,
        method: String,
        uri: String,
        headers: HashMap<String, Vec<String>>,
        body: Option<Vec<u8>>,
        timeout_ms: u64,
    ) -> Result<PhpResponse> {
        FfiWorkerPool::handle_http_request(self, method, uri, headers, body, timeout_ms)
            .await
            .map_err(map_embed_err)
    }

    async fn recycle_workers(&mut self) -> Result<()> {
        self.shutdown().await;
        FfiWorkerPool::initialize(self)
            .await
            .map_err(map_embed_err)?;
        Ok(())
    }

    async fn shutdown(&mut self) {
        FfiWorkerPool::shutdown(self).await;
    }
}
