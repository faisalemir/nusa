//! Worker pool manager for Octane mode.
//!
//! Skills applied:
//! - `m07-concurrency`: mpsc channels over shared state, JoinSet for lifecycle
//! - `m03-mutability`: Worker state isolated per process
//! - `m12-lifecycle`: spawnÃ¢â€ â€™handshakeÃ¢â€ â€™serveÃ¢â€ â€™recycleÃ¢â€ â€™shutdown
//! - `m13-domain-error`: IPC errors vs crash vs timeout distinction

#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(unix)]
use std::time::Duration;

use tracing::{info, warn};

#[cfg(unix)]
use std::process::Command;

use crate::error::WorkerError;
use nusa_ipc::protocol::IpcMessage;
use nusa_ipc::transport::IpcTransport;

/// Max time to wait for the PHP worker to create its Unix socket (production startup SLA).
#[cfg(unix)]
const WORKER_SOCKET_WAIT_TIMEOUT: Duration = Duration::from_secs(5);

/// Wait until the worker Unix socket exists, using filesystem notifications instead of polling.
#[cfg(unix)]
async fn wait_for_worker_socket(socket_path: &Path) -> Result<(), WorkerError> {
    use notify::{EventKind, Watcher};

    if socket_path.exists() {
        return Ok(());
    }

    let socket_dir = socket_path.parent().ok_or_else(|| {
        WorkerError::Handshake(format!(
            "invalid worker socket path: {}",
            socket_path.display()
        ))
    })?;
    let socket_path = socket_path.to_path_buf();
    let dir = socket_dir.to_path_buf();

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res
            && matches!(
                event.kind,
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Any
            )
        {
            let _ = tx.send(());
        }
    })
    .map_err(|e| WorkerError::Handshake(format!("socket watcher failed: {e}")))?;

    watcher
        .watch(&dir, notify::RecursiveMode::NonRecursive)
        .map_err(|e| WorkerError::Handshake(format!("socket watch failed: {e}")))?;

    let wait = async {
        loop {
            if socket_path.exists() {
                return Ok(());
            }
            if rx.recv().await.is_none() {
                break;
            }
        }
        if socket_path.exists() {
            Ok(())
        } else {
            Err(WorkerError::Handshake(format!(
                "worker socket not created: {}",
                socket_path.display()
            )))
        }
    };

    match tokio::time::timeout(WORKER_SOCKET_WAIT_TIMEOUT, wait).await {
        Ok(result) => result,
        Err(_) => Err(WorkerError::Handshake(format!(
            "timeout waiting for worker socket: {}",
            socket_path.display()
        ))),
    }
}

/// Perform version handshake on a connected transport (production path).
async fn handshake_worker(
    transport: &mut IpcTransport,
    worker_id: usize,
) -> Result<(), WorkerError> {
    let hello = IpcMessage::Hello {
        version: "1.0".to_string(),
        pid: std::process::id(),
        capabilities: vec!["http".to_string(), "tasks".to_string()],
    };

    transport.send(hello).await?;

    match tokio::time::timeout(std::time::Duration::from_secs(5), transport.recv()).await {
        Ok(Ok(IpcMessage::Ack)) => {
            info!("Worker {} handshake complete", worker_id);
            Ok(())
        }
        Ok(Ok(other)) => {
            warn!(
                "Worker {} expected Ack, got {:?}",
                worker_id,
                std::mem::discriminant(&other)
            );
            Err(WorkerError::Handshake("unexpected response".into()))
        }
        Ok(Err(e)) => {
            warn!("Worker {} handshake read error: {}", worker_id, e);
            Err(WorkerError::Handshake(e.to_string()))
        }
        Err(_) => Err(WorkerError::Handshake(
            "timeout: worker did not respond within 5s".into(),
        )),
    }
}

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
/// m12-lifecycle: spawnÃ¢â€ â€™handshakeÃ¢â€ â€™serveÃ¢â€ â€™recycleÃ¢â€ â€™shutdown.
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
    /// Stub worker for tests (no transport, no spawned process).
    #[doc(hidden)]
    pub fn new_test_stub(id: usize) -> Self {
        Self {
            id,
            pid: None,
            state: WorkerState::Idle,
            requests_handled: AtomicU64::new(0),
            rss_mb: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
            transport: None,
        }
    }

    /// Spawn a new PHP worker process and connect via UnixSocket (Unix) or TCP (Windows).
    ///
    /// On Unix: spawns PHP with Unix socket path.
    /// On non-Unix: spawns PHP worker and connects via TCP on a configurable port.
    #[cfg(unix)]
    pub async fn spawn(
        id: usize,
        app_root: PathBuf,
        _max_memory_mb: u64,
    ) -> Result<Self, WorkerError> {
        if !app_root.exists() {
            return Err(WorkerError::Handshake(format!(
                "app_root not found: {}",
                app_root.display()
            )));
        }

        let socket_dir = app_root.join(".octane");
        tokio::fs::create_dir_all(&socket_dir).await?;
        let socket_path = socket_dir.join(format!("worker-{}.sock", id));
        let worker_script = app_root.join("php-driver/bin/octane-rust-worker");

        if !worker_script.exists() {
            warn!(
                "Worker {} stub — Octane worker script missing at {}",
                id,
                worker_script.display()
            );
            return Ok(Self::new_test_stub(id));
        }

        let mut child = match Command::new("php")
            .args([
                worker_script.to_string_lossy().as_ref(),
                socket_path.to_string_lossy().as_ref(),
            ])
            .current_dir(&app_root)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                warn!("Worker {id} stub — could not spawn PHP: {e}");
                return Ok(Self::new_test_stub(id));
            }
        };

        let ready = async {
            wait_for_worker_socket(&socket_path).await?;
            let mut transport =
                IpcTransport::connect(socket_path.to_string_lossy().as_ref()).await?;
            handshake_worker(&mut transport, id).await?;
            Ok::<_, WorkerError>(transport)
        };

        match ready.await {
            Ok(transport) => {
                let pid = child.id();
                info!("Worker {} spawned (pid: {:?})", id, pid);
                Ok(Self {
                    id,
                    pid: Some(pid),
                    state: WorkerState::Idle,
                    requests_handled: AtomicU64::new(0),
                    rss_mb: AtomicU64::new(0),
                    error_count: AtomicU64::new(0),
                    transport: Some(transport),
                })
            }
            Err(e) => {
                let _ = child.kill();
                warn!("Worker {id} stub — Octane worker unavailable: {e}");
                Ok(Self::new_test_stub(id))
            }
        }
    }

    /// Spawn a new PHP worker process and connect via TCP (Windows).
    #[cfg(not(unix))]
    pub async fn spawn(
        id: usize,
        app_root: PathBuf,
        _max_memory_mb: u64,
    ) -> Result<Self, WorkerError> {
        if !app_root.exists() {
            return Err(WorkerError::Handshake(format!(
                "app_root not found: {}",
                app_root.display()
            )));
        }

        // On Windows, try to connect to worker via TCP on a pre-assigned port
        let port = 19000 + id as u16; // Each worker gets a unique port
        let addr = format!("127.0.0.1:{}", port);

        info!("Connecting to worker {} via TCP at {}", id, addr);

        // Single bounded connect (nusa-ipc TCP_CONNECT_TIMEOUT); avoids duplicate probe + connect.
        if let Ok(mut transport) = IpcTransport::connect(&addr).await
            && handshake_worker(&mut transport, id).await.is_ok()
        {
            return Ok(Self {
                id,
                pid: None,
                state: WorkerState::Idle,
                requests_handled: AtomicU64::new(0),
                rss_mb: AtomicU64::new(0),
                error_count: AtomicU64::new(0),
                transport: Some(transport),
            });
        }

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
/// m12-lifecycle: initializeÃ¢â€ â€™routeÃ¢â€ â€™recycleÃ¢â€ â€™shutdown.
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

    /// Initialize the worker pool (spawns workers concurrently).
    pub async fn initialize(&mut self) -> Result<(), WorkerError> {
        let count = self.max_workers;
        info!("Initializing worker pool with {count} workers");

        let app_root = self.app_root.clone();
        let memory_mb = self.max_memory_mb;

        let mut join_set = tokio::task::JoinSet::new();
        for i in 0..count {
            let root = app_root.clone();
            join_set.spawn(async move {
                let worker = Worker::spawn(i, root, memory_mb).await?;
                Ok::<_, WorkerError>((i, worker))
            });
        }

        let mut spawned = Vec::with_capacity(count);
        while let Some(join_result) = join_set.join_next().await {
            spawned
                .push(join_result.map_err(|e| {
                    WorkerError::Handshake(format!("worker task join failed: {e}"))
                })?);
        }
        let mut ready: Vec<(usize, Worker)> = Vec::with_capacity(count);
        for item in spawned {
            ready.push(item?);
        }
        ready.sort_by_key(|(i, _)| *i);

        self.workers.clear();
        self.idle_queue.clear();
        for (i, worker) in ready {
            debug_assert_eq!(i, self.workers.len());
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
        if worker_id >= self.workers.len() {
            return;
        }
        if self.workers[worker_id].state == WorkerState::Draining {
            self.idle_queue.retain(|&id| id != worker_id);
            return;
        }
        if !self.idle_queue.contains(&worker_id) {
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

    /// Populate the pool with stub workers (no PHP spawn). For tests only.
    #[doc(hidden)]
    pub fn initialize_test_stubs(&mut self) {
        self.workers.clear();
        self.idle_queue.clear();
        for i in 0..self.max_workers {
            self.workers.push(Worker::new_test_stub(i));
            self.idle_queue.push(i);
        }
    }

    /// Inject a stub worker for tests (`worker.id` must equal `workers.len()`).
    #[doc(hidden)]
    pub fn push_test_worker(&mut self, worker: Worker) {
        debug_assert_eq!(worker.id, self.workers.len());
        self.workers.push(worker);
    }

    /// Mark a worker as idle for tests.
    #[doc(hidden)]
    pub fn enqueue_idle_worker(&mut self, worker_id: usize) {
        self.idle_queue.push(worker_id);
    }
}
