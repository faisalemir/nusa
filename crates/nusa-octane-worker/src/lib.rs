//! Octane worker pool manager and IPC protocol.
//!
//! Skills applied:
//! - `m07-concurrency`: mpsc channels, JoinSet for lifecycle
//! - `m03-mutability`: Worker state isolated per process
//! - `m12-lifecycle`: spawn→handshake→serve→recycle→shutdown
//! - `m13-domain-error`: IPC errors vs crash vs timeout distinction

#![deny(unsafe_code)]
#![warn(clippy::all)]
#![allow(missing_docs)]

pub mod error;
pub mod metrics;
pub mod pool;
pub mod state_reset;
#[doc(hidden)]
pub mod test_fake_ipc;

pub use error::WorkerError;
pub use pool::WorkerPool;
pub use state_reset::StateResetOrchestrator;
