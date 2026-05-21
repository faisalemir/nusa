use std::io;

use nusa_ipc::IpcError;

/// Error type for Octane worker pool operations.
#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    /// IO error during transport.
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// IPC transport error.
    #[error("IPC error: {0}")]
    Ipc(#[from] IpcError),

    /// Handshake failed with worker.
    #[error("Handshake failed: {0}")]
    Handshake(String),

    /// Worker has no transport (stub mode).
    #[error("Worker {0} has no transport (stub mode)")]
    NoTransport(usize),
}
