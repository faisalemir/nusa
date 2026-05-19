use thiserror::Error;

/// Domain-specific errors mapped to HTTP status codes (m06-error-handling)
#[derive(Error, Debug)]
pub enum EngineError {
    #[error("execution timeout")]
    Timeout, // -> 408

    #[error("sandbox violation: {0}")]
    Sandbox(String), // -> 500

    #[error("php fatal: {0}")]
    PhpFatal(String), // -> 502

    #[error("resource limit exceeded")]
    ResourceLimit, // -> 429 or 503

    #[error("plugin error: {0}")]
    Plugin(String), // -> 500

    #[error("ipc protocol error: {0}")]
    IpcProtocol(String), // -> 500
}

impl EngineError {
    /// Map EngineError to HTTP Status Code
    pub fn to_http_status(&self) -> u16 {
        match self {
            Self::Timeout => 408,
            Self::Sandbox(_) => 500,
            Self::PhpFatal(_) => 502,
            Self::ResourceLimit => 429,
            Self::Plugin(_) => 500,
            Self::IpcProtocol(_) => 500,
        }
    }
}

pub type Result<T> = std::result::Result<T, EngineError>;
