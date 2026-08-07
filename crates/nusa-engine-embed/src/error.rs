//! Embed worker pool errors.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum EmbedError {
    #[error("handshake failed: {0}")]
    Handshake(String),

    #[error("worker {0}: {1}")]
    Worker(usize, String),

    #[error("pool not ready")]
    NotReady,

    #[error("no idle workers")]
    NoIdleWorker,

    #[error("io error: {0}")]
    Io(String),

    #[error("json error: {0}")]
    Json(String),
}

impl EmbedError {
    pub fn handshake(msg: impl Into<String>) -> Self {
        Self::Handshake(msg.into())
    }
}
