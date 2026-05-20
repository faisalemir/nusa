//! Core types, traits, and errors for the Nusa PHP runtime.
//!
//! Skills applied:
//! - `unsafe-checker`: #![deny(unsafe_code)] enforced
//! - `m09-domain`: Domain model types (RequestContext, PhpResponse)
//! - `m06-error-handling`: thiserror-based error taxonomy
//! - `m05-type-driven`: Newtype wrappers (TraceId, TenantId, WorkerId)
//! - `m04-zero-cost`: PhpEngine trait for engine dispatch
//! - `coding-guidelines`: No get_ prefix, snake_case, meaningful names

#![deny(unsafe_code)]
#![warn(clippy::all)]
#![allow(missing_docs)]

pub mod engine;
pub mod error;
pub mod guards;
pub mod rate_limiter;
pub mod task;
pub mod tenant;
pub mod types;
pub mod vfs;

pub use engine::*;
pub use error::*;
pub use guards::*;
pub use rate_limiter::*;
pub use task::*;
pub use tenant::*;
pub use types::*;
pub use vfs::*;
