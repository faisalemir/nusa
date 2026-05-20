//! PHP child process engine.
//!
//! Skills applied:
//! - `unsafe-checker`: #![deny(unsafe_code)] enforced
//! - `m07-concurrency`: tokio::process for async child management
//! - `m06-error-handling`: Process errors propagate properly
//! - `m12-lifecycle`: spawn → communicate → kill phases

#![deny(unsafe_code)]
#![warn(clippy::all)]
#![allow(missing_docs)]

pub mod engine;
pub mod process;

pub use engine::ChildEngine;
