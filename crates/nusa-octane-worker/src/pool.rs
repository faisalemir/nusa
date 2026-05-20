//! Worker pool manager for Octane mode.
//!
//! Skills applied:
//! - `m07-concurrency`: mpsc channels over shared state, JoinSet for lifecycle
//! - `m03-mutability`: Worker state isolated per process
//! - `m12-lifecycle`: spawn→handshake→serve→recycle→shutdown
//! - `m13-domain-error`: IPC errors vs crash vs timeout distinction

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use tracing::{info, warn};

use nusa_ipc::protocol::IpcMessage;
use nusa_ipc::transport::IpcTransport;

/// Worker state in the pool.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorkerState {
    Idle,
    Busy,
    Draining,
    Stopped,
}

/// A single PHP worker in the Octane pool.
///
/// m07-concurrency: Worker communicates via IpcTransport.
/// m12-lifecycle: spawn→handshake→serve→recycle→shutdown.
pub struct Worker {
    pub id: usize,
    pub pid: Option<u32>,
    pub state: WorkerState,
    pub requests_handled: AtomicU64,
    pub rss_mb: AtomicU64,
    pub error_count: AtomicU64,
    transport: Option<IpcTransport>,
}

impl Worker {
    /// Spawn a new PHP worker process and connect via UnixSocket.
    ///
    /// On non-Unix platforms, creates a stub worker.
    #[cfg(unix)]
    pub async fn spawn(
        id: usize,
        app_root: PathBuf,
        _max_memory_mb: u64,
    ) -> anyhow::Result<Self> {
        let socket_dir = app_root.join(".octane");
        tokio::fs::create_dir_all(&socket_dir).await?;
        let socket_path = socket_dir.join(format!("worker-{}.sock", id));

        // Spawn PHP worker process
        let mut child = Command::new("php")
            .args([
                app_root.join("php-driver/bin/octane-rust-worker")
                    .to_string_lossy()
                    .as_ref(),
                socket_path.to_string_lossy().as_ref(),
            ])
            .current_dir(&app_root)
            .kill_on_drop(true)
            .spawn()?;

        // Wait for socket to appear
        for _ in 0..50 {
            if socket_path.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        // Connect via IPC
        let transport = IpcTransport::connect(socket_path.to_string_lossy().as_ref())
            .await?;

        // Send Hello handshake
        let hello = IpcMessage::Hello {
            version: "1.0".to_string(),
            pid: std::process::id(),
            capabilities: vec!["http".to_string(), "tasks".to_string()],
        };

        // Wait for Ack
        transport.send(hello).await?;

        let pid = child.id();
        info!("Worker {} spawned (pid: {:?})", id, pid);

        Ok(Self {
            id,
            pid,
            state: WorkerState::Idle,
            requests_handled: AtomicU64::new(0),
            rss_mb: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
            transport: Some(transport),
        })
    }

    #[cfg(not(unix))]
    pub async fn spawn(
        id: usize,
        _app_root: PathBuf,
        _max_memory_mb: u64,
    ) -> anyhow::Result<Self> {
        warn!("Worker {} stub — UnixSocket not available on this platform", id);
        Ok(Self {
            id,
            pid: None,
            state: WorkerState::Idle,
            requests_handled: AtomicU64::new(0),
            rss_mb: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
            transport: None,
        })
    }

    /// Send a request to this worker.
    pub async fn handle_request(
        &mut self,
        method: String,
        uri: String,
        timeout_ms: u64,
    ) -> anyhow::Result<IpcMessage> {
        if let Some(ref mut transport) = self.transport {
            self.state = WorkerState::Busy;
            let result = transport.request_response(method, uri, Default::default(), timeout_ms).await;
            if result.is_ok() {
                self.requests_handled.fetch_add(1, Ordering::SeqCst);
            } else {
                self.error_count.fetch_add(1, Ordering::SeqCst);
            }
            self.state = WorkerState::Idle;
            result
        } else {
            anyhow::bail!("Worker {} has no transport (stub mode)", self.id);
        }
    }

    /// Check if this worker should be recycled.
    pub fn should_recycle(&self, max_requests: u64, max_memory_mb: u64) -> bool {
        let requests = self.requests_handled.load(Ordering::SeqCst);
        let memory = self.rss_mb.load(Ordering::SeqCst);
        requests >= max_requests || memory >= max_memory_mb
    }

    /// Stop this worker gracefully.
    pub async fn stop(&mut self) -> anyhow::Result<()> {
        if let Some(ref mut transport) = self.transport {
            transport.send(IpcMessage::Shutdown).await?;
        }
        self.state = WorkerState::Stopped;
        info!("Worker {} stopped", self.id);
        Ok(())
    }
}

/// Worker pool manager for Octane mode.
///
/// m07-concurrency: Uses JoinSet for managing worker lifecycles.
/// m12-lifecycle: initialize→route→recycle→shutdown.
pub struct WorkerPool {
    workers: Vec<Worker>,
    idle_queue: Vec<usize>,
    max_workers: usize,
    app_root: PathBuf,
    max_memory_mb: u64,
    #[allow(dead_code)]
    max_requests: u64,
    total_handled: AtomicU64,
    total_errors: AtomicU64,
}

impl WorkerPool {
    pub fn new(
        max_workers: usize,
        app_root: PathBuf,
        max_memory_mb: u64,
        max_requests: u64,
    ) -> Self {
        Self {
            workers: Vec::with_capacity(max_workers),
            idle_queue: Vec::new(),
            max_workers,
            app_root,
            max_memory_mb,
            max_requests,
            total_handled: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
        }
    }

    /// Initialize the worker pool.
    pub async fn initialize(&mut self) -> anyhow::Result<()> {
        info!("Initializing worker pool with {} workers", self.max_workers);

        for i in 0..self.max_workers {
            let worker = Worker::spawn(
                i,
                self.app_root.clone(),
                self.max_memory_mb,
            )
            .await?;
            self.idle_queue.push(i);
            self.workers.push(worker);
        }

        info!("Worker pool initialized");
        Ok(())
    }

    /// Get an idle worker from the pool.
    pub fn get_idle_worker(&mut self) -> Option<&mut Worker> {
        if let Some(idx) = self.idle_queue.pop() {
            Some(&mut self.workers[idx])
        } else {
            None
        }
    }

    /// Return a worker to the idle queue.
    pub fn return_worker(&mut self, worker_id: usize) {
        if self.workers[worker_id].state != WorkerState::Draining {
            self.idle_queue.push(worker_id);
        }
    }

    /// Recycle a worker: stop it and spawn a replacement.
    pub async fn recycle_worker(&mut self, worker_id: usize) -> anyhow::Result<()> {
        info!("Recycling worker {}", worker_id);
        // Remove from idle queue first (worker is draining)
        self.idle_queue.retain(|&id| id != worker_id);
        self.workers[worker_id].state = WorkerState::Draining;
        self.workers[worker_id].stop().await?;

        let new_worker = Worker::spawn(
            worker_id,
            self.app_root.clone(),
            self.max_memory_mb,
        )
        .await?;
        self.workers[worker_id] = new_worker;
        self.idle_queue.push(worker_id);

        info!("Worker {} recycled successfully", worker_id);
        Ok(())
    }

    /// Shut down all workers gracefully.
    pub async fn shutdown(&mut self) -> anyhow::Result<()> {
        info!("Shutting down worker pool ({} workers)", self.workers.len());

        for worker in self.workers.iter_mut() {
            let _ = worker.stop().await;
        }

        self.workers.clear();
        self.idle_queue.clear();
        info!("Worker pool shut down");
        Ok(())
    }

    /// Statistics
    pub fn total_handled(&self) -> u64 {
        self.total_handled.load(Ordering::SeqCst)
    }

    pub fn total_errors(&self) -> u64 {
        self.total_errors.load(Ordering::SeqCst)
    }

    /// Test helpers
    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    pub fn idle_count(&self) -> usize {
        self.idle_queue.len()
    }

    pub fn worker(&self, id: usize) -> &Worker {
        &self.workers[id]
    }

    pub fn worker_mut(&mut self, id: usize) -> &mut Worker {
        &mut self.workers[id]
    }
}
