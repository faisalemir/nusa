//! PHP WASM sandbox engine (wasmtime).
//!
//! Skills applied:
//! - `unsafe-checker`: #![deny(unsafe_code)] enforced
//! - `m03-mutability`: StoreLimits interior mutability
//! - `m06-error-handling`: WASM trap → EngineError::Sandbox
//! - `m12-lifecycle`: init → execute → shutdown phases

#![deny(unsafe_code)]
#![warn(clippy::all)]

pub mod engine;
pub mod runtime;

pub use engine::WasmEngine;
