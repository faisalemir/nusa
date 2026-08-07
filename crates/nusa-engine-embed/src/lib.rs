//! Embedded PHP worker pool for Laravel Octane mode.
//!
//! Production target: libphp ZTS in-process (`embed-php` feature).
//! Default build: long-lived `php nusa_embed_daemon.php` over stdin/stdout (no UDS).

#![deny(unsafe_code)]
#![warn(clippy::all)]
#![allow(missing_docs)]

pub mod error;
pub mod frame;
pub mod laravel_runtime_impl;
pub mod paths;
pub mod pool;
pub mod stdio_worker;

pub use error::EmbedError;
pub use pool::FfiWorkerPool;
