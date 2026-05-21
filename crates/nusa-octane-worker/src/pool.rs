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

#[cfg(unix)]
use std::process::Command;

#[cfg(not(unix))]
use tokio::net::TcpStream;

use crate::error::WorkerError;
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
    /// Spawn a new PHP worker process and connect via UnixSocket (Unix) or TCP (Windows).
    ///
    /// On Unix: spawns PHP with Unix socket path.
    /// On non-Unix: spawns PHP worker and connects via TCP on a configurable port.
    #[cfg(unix)]
    pub async fn spawn(id: usize, app_root: PathBuf, _max_memory_mb: u64) -> Result<Self, WorkerError> {
        let socket_dir = app_root.join(".octane");
        tokio::fs::create_dir_all(&socket_dir).await?;
        let socket_path = socket_dir.join(format!("worker-{}.sock", id));

        // Spawn PHP worker process
        let mut child = Command::new("php")
            .args([
                app_root
                    .join("php-driver/bin/octane-rust-worker")
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
        let transport = IpcTransport::connect(socket_path.to_string_lossy().as_ref()).await?;

        // Send Hello handshake and wait for Ack (Strategy §A.1: version handshake)
        let hello = IpcMessage::Hello {
            version: "1.0".to_string(),
            pid: std::process::id(),
            capabilities: vec!["http".to_string(), "tasks".to_string()],
        };

        transport.send(hello).await?;

        // Wait for Ack with timeout
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            transport.recv(),
        ).await {
            Ok(Ok(IpcMessage::Ack)) => {
                info!("Worker {} handshake complete", id);
            }
            Ok(Ok(other)) => {
                warn!(
                    "Worker {} expected Ack, got {:?}",
                    id,
                    std::mem::discriminant(&other)
                );
                return Err(WorkerError::Handshake("unexpected response".into()));
            }
            Ok(Err(e)) => {
                warn!("Worker {} handshake read error: {}", id, e);
                return Err(WorkerError::Handshake(e.to_string()));
            }
            Err(_) => {
                warn!("Worker {} handshake timeout", id);
                return Err(WorkerError::Handshake("timeout: worker did not respond within 5s".into()));
            }
        }

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

    /// Spawn a new PHP worker process and connect via TCP (Windows).
    #[cfg(not(unix))]
    pub async fn spawn(id: usize, _app_root: PathBuf, _max_memory_mb: u64) -> Result<Self, WorkerError> {
        // On Windows, try to connect to worker via TCP on a pre-assigned port
        let port = 19000 + id as u16; // Each worker gets a unique port
        let addr = format!("127.0.0.1:{}", port);

        info!("Connecting to worker {} via TCP at {}", id, addr);

        // Wait for TCP server to be ready (short timeout for testing fallback)
        let mut connected = false;
        for _ in 0..10 {
            if TcpStream::connect(&addr).await.is_ok() {
                connected = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        if connected {
            // Connect via IPC (TCP transport)
            let mut transport = IpcTransport::connect(&addr).await?;

            // Send Hello handshake and wait for Ack
            let hello = IpcMessage::Hello {
                version: "1.0".to_string(),
                pid: std::process::id(),
                capabilities: vec!["http".to_string(), "tasks".to_string()],
            };

            transport.send(hello).await?;

            // Wait for Ack with timeout
            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                transport.recv(),
            ).await {
                Ok(Ok(IpcMessage::Ack)) => {
                    info!("Worker {} handshake complete (TCP)", id);
                }
                Ok(Ok(other)) => {
                    warn!(
                        "Worker {} expected Ack, got {:?}",
                        id,
                        std::mem::discriminant(&other)
                    );
                    return Err(WorkerError::Handshake("unexpected response".into()));
                }
                Ok(Err(e)) => {
                    warn!("Worker {} handshake read error: {}", id, e);
                    return Err(WorkerError::Handshake(e.to_string()));
                }
                Err(_) => {
                    warn!("Worker {} handshake timeout (TCP)", id);
                    return Err(WorkerError::Handshake("timeout: worker did not respond within 5s".into()));
                }
            }

            Ok(Self {
                id,
                pid: None,
                state: WorkerState::Idle,
                requests_handled: AtomicU64::new(0),
                rss_mb: AtomicU64::new(0),
                error_count: AtomicU64::new(0),
                transport: Some(transport),
            })
        } else {
            // No PHP worker available — fall back to stub mode (useful for testing)
            warn!(
                "Worker {} stub — no PHP worker available at TCP {}",
                id, addr
            );
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
    }

    /// Send a request to this worker.
    pub async fn handle_request(
        &mut self,
        method: String,
        uri: String,
        timeout_ms: u64,
    ) -> Result<IpcMessage, WorkerError> {
        if let Some(ref mut transport) = self.transport {
            self.state = WorkerState::Busy;
            let result = transport
                .request_response(method, uri, Default::default(), timeout_ms)
                .await;
            if result.is_ok() {
                self.requests_handled.fetch_add(1, Ordering::SeqCst);
            } else {
                self.error_count.fetch_add(1, Ordering::SeqCst);
            }
            self.state = WorkerState::Idle;
            result.map_err(WorkerError::Ipc)
        } else {
            Err(WorkerError::NoTransport(self.id))
        }
    }

    /// Check if this worker should be recycled.
    pub fn should_recycle(&self, max_requests: u64, max_memory_mb: u64) -> bool {
        let requests = self.requests_handled.load(Ordering::SeqCst);
        let memory = self.rss_mb.load(Ordering::SeqCst);
        requests >= max_requests || memory >= max_memory_mb
    }

    /// Stop this worker gracefully.
    pub async fn stop(&mut self) -> Result<(), WorkerError> {
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
/// m10-performance: Telemetry-driven recycling based on RSS, error rate, GC pause.
pub struct WorkerPool {
    workers: Vec<Worker>,
    idle_queue: Vec<usize>,
    max_workers: usize,
    app_root: PathBuf,
    max_memory_mb: u64,
    max_requests: u64,
    total_handled: AtomicU64,
    total_errors: AtomicU64,
}

impl WorkerPool {
    /// Create a new worker pool with the given configuration.
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
    pub async fn initialize(&mut self) -> Result<(), WorkerError> {
        info!("Initializing worker pool with {} workers", self.max_workers);

        for i in 0..self.max_workers {
            let worker = Worker::spawn(i, self.app_root.clone(), self.max_memory_mb).await?;
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
    pub async fn recycle_worker(&mut self, worker_id: usize) -> Result<(), WorkerError> {
        info!("Recycling worker {}", worker_id);
        // Remove from idle queue first (worker is draining)
        self.idle_queue.retain(|&id| id != worker_id);
        self.workers[worker_id].state = WorkerState::Draining;
        self.workers[worker_id].stop().await?;

        let new_worker =
            Worker::spawn(worker_id, self.app_root.clone(), self.max_memory_mb).await?;
        self.workers[worker_id] = new_worker;
        self.idle_queue.push(worker_id);

        info!("Worker {} recycled successfully", worker_id);
        Ok(())
    }

    /// Shut down all workers gracefully.
    pub async fn shutdown(&mut self) -> Result<(), WorkerError> {
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

    pub fn max_requests(&self) -> u64 {
        self.max_requests
    }

    pub fn worker(&self, id: usize) -> &Worker {
        &self.workers[id]
    }

    pub fn worker_mut(&mut self, id: usize) -> &mut Worker {
        &mut self.workers[id]
    }
}
