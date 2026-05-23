//! Embed worker pool (`FfiWorkerPool`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use nusa_core::async_io::bridge_from_env;
use nusa_core::{AsyncIoBridge, PhpResponse};

use crate::error::EmbedError;
use crate::stdio_worker::StdioEmbedWorker;

/// Pool of long-lived embed PHP workers (stdio daemon or future in-process FFI).
pub struct FfiWorkerPool {
    max_workers: usize,
    standby_count: usize,
    app_root: PathBuf,
    php_binary: String,
    #[allow(dead_code)] // reserved for RSS-based recycle (post-GA)
    max_memory_mb: u64,
    max_requests: u64,
    workers: Vec<StdioEmbedWorker>,
    standby: Vec<StdioEmbedWorker>,
    idle_queue: Vec<usize>,
    total_handled: AtomicU64,
    total_errors: AtomicU64,
    async_io: Arc<dyn AsyncIoBridge>,
}

impl FfiWorkerPool {
    pub fn new(
        max_workers: usize,
        app_root: PathBuf,
        php_binary: String,
        max_memory_mb: u64,
        max_requests: u64,
    ) -> Self {
        Self::with_standby(
            max_workers,
            0,
            app_root,
            php_binary,
            max_memory_mb,
            max_requests,
        )
    }

    pub fn with_standby(
        max_workers: usize,
        standby_workers: usize,
        app_root: PathBuf,
        php_binary: String,
        max_memory_mb: u64,
        max_requests: u64,
    ) -> Self {
        Self {
            max_workers,
            standby_count: standby_workers,
            app_root,
            php_binary,
            max_memory_mb,
            max_requests,
            workers: Vec::new(),
            standby: Vec::new(),
            idle_queue: Vec::new(),
            total_handled: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
            async_io: Arc::from(bridge_from_env()),
        }
    }

    pub async fn initialize(&mut self) -> Result<(), EmbedError> {
        let count = self.max_workers;
        tracing::info!(
            "Initializing embed worker pool with {count} workers (standby={})",
            self.standby_count
        );

        self.workers.clear();
        self.standby.clear();
        self.idle_queue.clear();

        for i in 0..count {
            let worker = StdioEmbedWorker::spawn(
                i,
                self.app_root.clone(),
                self.php_binary.as_str(),
                self.async_io.clone(),
            )
            .await?;
            self.idle_queue.push(i);
            self.workers.push(worker);
        }

        for offset in 0..self.standby_count {
            let id = self.max_workers + offset;
            let worker = StdioEmbedWorker::spawn(
                id,
                self.app_root.clone(),
                self.php_binary.as_str(),
                self.async_io.clone(),
            )
            .await?;
            self.standby.push(worker);
        }

        if self.max_workers > 0 && !self.is_ready() {
            return Err(EmbedError::handshake(
                "embed pool not ready: worker bootstrap failed",
            ));
        }

        tracing::info!("Embed worker pool initialized");
        Ok(())
    }

    pub fn is_ready(&self) -> bool {
        if self.max_workers == 0 {
            return true;
        }
        let active =
            self.workers.len() == self.max_workers && self.workers.iter().all(|w| w.is_booted());
        if self.standby_count == 0 {
            return active;
        }
        active
            && self.standby.len() == self.standby_count
            && self.standby.iter().all(|w| w.is_booted())
    }

    pub fn configured_workers(&self) -> usize {
        self.max_workers
    }

    pub async fn handle_http_request(
        &mut self,
        method: String,
        uri: String,
        headers: HashMap<String, Vec<String>>,
        body: Option<Vec<u8>>,
        _timeout_ms: u64,
    ) -> Result<PhpResponse, EmbedError> {
        if !self.is_ready() {
            return Err(EmbedError::NotReady);
        }

        let worker_id = self.idle_queue.pop().ok_or(EmbedError::NoIdleWorker)?;

        let result = self.workers[worker_id]
            .handle_request(method, uri, headers, body)
            .await;

        self.return_worker(worker_id);

        match result {
            Ok(res) => {
                self.total_handled.fetch_add(1, Ordering::SeqCst);
                if self.workers[worker_id].requests_handled() >= self.max_requests {
                    self.recycle_worker(worker_id).await?;
                }
                Ok(res)
            }
            Err(e) => {
                self.total_errors.fetch_add(1, Ordering::SeqCst);
                Err(e)
            }
        }
    }

    fn return_worker(&mut self, worker_id: usize) {
        if worker_id < self.workers.len() && !self.idle_queue.contains(&worker_id) {
            self.idle_queue.push(worker_id);
        }
    }

    async fn recycle_worker(&mut self, worker_id: usize) -> Result<(), EmbedError> {
        if worker_id >= self.workers.len() {
            return Ok(());
        }
        self.workers[worker_id].shutdown().await;

        if let Some(replacement) = self.standby.pop() {
            self.workers[worker_id] = replacement;
            tracing::info!("Embed worker {worker_id} recycled via warm standby");
            self.replenish_standby().await?;
            return Ok(());
        }

        let worker = StdioEmbedWorker::spawn(
            worker_id,
            self.app_root.clone(),
            &self.php_binary,
            self.async_io.clone(),
        )
        .await?;
        self.workers[worker_id] = worker;
        Ok(())
    }

    async fn replenish_standby(&mut self) -> Result<(), EmbedError> {
        while self.standby.len() < self.standby_count {
            let id = self.max_workers + self.standby.len();
            let worker = StdioEmbedWorker::spawn(
                id,
                self.app_root.clone(),
                &self.php_binary,
                self.async_io.clone(),
            )
            .await?;
            self.standby.push(worker);
        }
        Ok(())
    }

    pub async fn shutdown(&mut self) {
        for worker in self.workers.iter_mut().chain(self.standby.iter_mut()) {
            worker.shutdown().await;
        }
        self.workers.clear();
        self.standby.clear();
        self.idle_queue.clear();
    }
}
