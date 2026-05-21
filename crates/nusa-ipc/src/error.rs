use std::io;

/// Error type for IPC operations.
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    /// IO error during transport.
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// Serialization or deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Framing error (invalid payload, incomplete frame).
    #[error("Framing error: {0}")]
    Framing(String),

    /// Connection failed.
    #[error("Connection failed: {0}")]
    Connection(String),

    /// Handshake failed (unexpected response or timeout).
    #[error("Handshake failed: {0}")]
    Handshake(String),

    /// Transport is not available (e.g. stub mode).
    #[error("Transport not available: {0}")]
    TransportNotAvailable(String),

    /// Worker missed consecutive heartbeats.
    #[error("Worker missed {0} consecutive heartbeats")]
    MissedHeartbeats(u32),
}
