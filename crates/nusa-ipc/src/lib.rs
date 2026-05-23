//! IPC Contract v1: Framed binary codec for Rust↔PHP communication.
//!
//! Skills applied:
//! - `m06-error-handling`: Framing errors as Results
//! - `m11-ecosystem`: tokio-util codec integration
//! - `unsafe-checker`: #![deny(unsafe_code)] enforced

#![deny(unsafe_code)]
#![warn(clippy::all)]
#![allow(missing_docs)]
#![allow(clippy::large_enum_variant, clippy::box_collection)]

pub mod cookies;
pub mod error;
pub mod framing;
pub mod protocol;
pub mod trace;
pub mod transport;

pub use error::IpcError;
pub use framing::*;
pub use protocol::*;
pub use trace::*;
