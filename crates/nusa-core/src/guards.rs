//! Resource guards for request lifecycle management
//!
//! Skills applied:
//! - `m07-concurrency`: Semaphore for backpressure, no Mutex needed
//! - `m13-domain-error`: Resource limit errors with context
//! - `domain-web`: Request size caps, timeout enforcement

use std::sync::Arc;

use tokio::sync::Semaphore;
use tokio::time::{Duration, timeout};

use crate::{EngineError, Result};

/// Resource guard configuration
///
/// Controls request-level resource limits to prevent abuse.
///
/// # Example
/// ```rust
/// use nusa_core::ResourceGuard;
///
/// // Default: 10MB max body, 30s timeout, 100 concurrent
/// let guard = ResourceGuard::default();
/// assert_eq!(guard.max_request_bytes, 10 * 1024 * 1024);
/// assert_eq!(guard.request_timeout_ms, 30_000);
/// assert_eq!(guard.max_concurrent, 100);
/// ```
#[derive(Debug, Clone)]
pub struct ResourceGuard {
    /// Maximum request body size in bytes
    pub max_request_bytes: usize,
    /// Per-request timeout duration
    pub request_timeout_ms: u64,
    /// Maximum concurrent requests (backpressure)
    pub max_concurrent: usize,
}

impl Default for ResourceGuard {
    fn default() -> Self {
        Self {
            max_request_bytes: 10 * 1024 * 1024, // 10MB
            request_timeout_ms: 30_000,          // 30s
            max_concurrent: 100,
        }
    }
}

/// Backpressure semaphore — limits concurrent request processing
///
/// m07-concurrency: Uses tokio::sync::Semaphore, not std::sync::Mutex.
/// When all permits are acquired, new requests receive 503 immediately.
pub struct BackpressureGuard {
    semaphore: Arc<Semaphore>,
}

impl BackpressureGuard {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    /// Try to acquire a permit for request processing.
    /// Returns None if the system is at capacity (backpressure).
    pub async fn try_acquire(&self) -> Option<tokio::sync::SemaphorePermit<'_>> {
        self.semaphore.try_acquire().ok()
    }
}

/// Execute a future with a timeout wrapper.
///
/// m13-domain-error: Returns EngineError::Timeout on timeout.
pub async fn with_timeout<F, T>(timeout_ms: u64, future: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    if timeout_ms == 0 {
        return Err(EngineError::Timeout);
    }
    match timeout(Duration::from_millis(timeout_ms), future).await {
        Ok(result) => result,
        Err(_elapsed) => Err(EngineError::Timeout),
    }
}

/// Validate that a request body does not exceed the size limit.
///
/// Returns true if the request is within limits.
pub fn validate_request_size(content_length: Option<u64>, max_bytes: usize) -> bool {
    if max_bytes == 0 {
        return false;
    }
    if let Some(len) = content_length {
        len <= max_bytes as u64
    } else {
        true
    }
}
