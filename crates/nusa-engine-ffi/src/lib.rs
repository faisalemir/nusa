//! PHP ZTS FFI engine wrapper (isolated unsafe).
//!
//! Skills applied:
//! - `unsafe-checker`: ONLY crate with #![allow(unsafe_code)]
//! - `m07-concurrency`: spawn_blocking for CPU-bound FFI
//! - `m06-error-handling`: catch_unwind for panic safety
//! - `m12-lifecycle`: init → execute → shutdown phases

#![allow(unsafe_code)]
#![warn(clippy::all)]
#![allow(missing_docs)]

pub mod engine;

pub use engine::FfiEngine;
