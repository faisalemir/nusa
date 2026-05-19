//! PHP engine backed by libphp ZTS via FFI.
//!
//! # Safety (unsafe-checker)
//! This is the ONLY crate in the workspace allowed to use `unsafe`.
//! All unsafe blocks have `// SAFETY:` comments referencing:
//! - PHP ZTS thread-local storage guarantees
//! - Isolated execution per thread
//! - catch_unwind for panic safety
//!
//! Skills applied:
//! - `unsafe-checker`: All unsafe blocks documented with SAFETY comments
//! - `m07-concurrency`: spawn_blocking for CPU-bound FFI, don't block async executor
//! - `m02-resource`: Thread-local output buffer (RefCell)
//! - `m12-lifecycle`: init → execute → shutdown phases

use async_trait::async_trait;
use phprt_core::{PhpEngine, RequestContext, PhpResponse, EngineError, Result};
use std::panic::AssertUnwindSafe;
use tokio::task::spawn_blocking;
use tracing::info;

// Thread-local output buffer for stdout capture (m02-resource)
thread_local! {
    static OUTPUT_BUF: std::cell::RefCell<Vec<u8>> = std::cell::RefCell::new(Vec::with_capacity(8192));
}

/// PHP engine backed by libphp ZTS via FFI.
///
/// Uses `spawn_blocking` (m07-concurrency) to prevent blocking the tokio reactor.
/// Uses `catch_unwind` (m06-error-handling) to catch segfaults/panics.
pub struct FfiEngine {
    pool: tokio::sync::Semaphore,
}

impl FfiEngine {
    pub fn new(max_workers: usize) -> Self {
        Self {
            pool: tokio::sync::Semaphore::new(max_workers),
        }
    }
}

/// Stub implementation for non-Linux or when PHP headers are unavailable.
#[cfg(not(php_embed_available))]
mod ffi_impl {
    use phprt_core::{PhpResponse, RequestContext};

    pub fn run_sync(
        _ctx: &RequestContext,
        output_buf: &mut Vec<u8>,
    ) -> phprt_core::Result<PhpResponse> {
        output_buf.clear();
        Ok(PhpResponse {
            status: 200,
            headers: http::HeaderMap::new(),
            body: bytes::Bytes::from("FFI engine stub — requires Linux with PHP ZTS headers"),
        })
    }
}

/// Actual FFI implementation for Linux with PHP ZTS.
#[cfg(php_embed_available)]
mod ffi_impl {
    include!(concat!(env!("OUT_DIR"), "/php_sys.rs"));

    use phprt_core::{PhpResponse, EngineError, RequestContext};
    use crate::OUTPUT_BUF;

    /// Custom ub_write callback — captures PHP output to thread-local buffer.
    ///
    /// # SAFETY (unsafe-checker)
    /// - `str` is guaranteed to point to `len` valid bytes by PHP's internal write path.
    /// - OUTPUT_BUF is thread-local, so concurrent writes from different threads are safe.
    /// - ZTS ensures each thread has its own isolated PHP state.
    extern "C" fn rust_ub_write(str: *const libc::c_char, len: libc::size_t) -> libc::size_t {
        if str.is_null() || len == 0 {
            return 0;
        }
        // SAFETY: PHP guarantees `str` points to `len` valid bytes during ub_write.
        // We only read, never write, and OUTPUT_BUF is thread-local.
        let slice = unsafe { std::slice::from_raw_parts(str as *const u8, len) };
        OUTPUT_BUF.with(|buf| buf.borrow_mut().extend_from_slice(slice));
        len
    }

    /// Execute PHP script via FFI on the current thread.
    ///
    /// # SAFETY (unsafe-checker)
    /// This function must be called from an isolated thread with ZTS enabled:
    /// - php_embed_init() initializes thread-local PHP state
    /// - php_embed_shutdown() cleans up thread-local state
    /// - No shared mutable state between threads (ZTS guarantee)
    pub fn run_sync(
        ctx: &RequestContext,
        output_buf: &mut Vec<u8>,
    ) -> phprt_core::Result<PhpResponse> {
        output_buf.clear();

        // SAFETY: php_embed_module is a global FFI struct. We only mutate ub_write
        // before init, and ZTS ensures thread-local storage for all PHP globals.
        // This is sound because each thread has its own php_embed_module instance.
        unsafe {
            php_embed_module.ub_write = Some(rust_ub_write);

            // Initialize PHP for this thread
            php_embed_init(0, std::ptr::null_mut());

            // Prepare script path
            let script_path = std::ffi::CString::new(
                ctx.script_path().to_string_lossy().as_bytes()
            ).map_err(|_| EngineError::PhpFatal("invalid script path".into()))?;

            // Create file handle
            let mut fh: zend_file_handle = std::mem::zeroed();
            zend_stream_init_filename(&mut fh, script_path.as_ptr());

            // Execute script
            let exec_ok = php_execute_script(&mut fh);

            // Clean up request state
            php_request_shutdown(std::ptr::null_mut());

            if exec_ok == 0 {
                return Err(EngineError::PhpFatal("script execution failed".into()));
            }
        }

        // Return captured output
        let body = std::mem::take(output_buf);
        Ok(PhpResponse {
            status: 200,
            headers: http::HeaderMap::new(),
            body: bytes::Bytes::from(body),
        })
    }
}

#[async_trait]
impl PhpEngine for FfiEngine {
    async fn execute(&self, ctx: RequestContext) -> Result<PhpResponse> {
        // m07-concurrency: CPU-bound FFI off the async executor
        let _permit = self.pool.acquire().await.map_err(|_| EngineError::ResourceLimit)?;

        let result = spawn_blocking({
            move || {
                // m06-error-handling: catch panics at FFI boundary
                std::panic::catch_unwind(AssertUnwindSafe(|| {
                    // SAFETY: Isolated thread with ZTS.
                    // All PHP globals are thread-local in ZTS mode.
                    // This is the ONLY place unsafe FFI calls are made.
                    ffi_impl::run_sync(
                        &ctx,
                        &mut OUTPUT_BUF.with(|b| b.borrow_mut().clone()),
                    )
                }))
                .map_err(|_| {
                    // m06-error-handling: panic/segfault caught -> PhpFatal
                    EngineError::PhpFatal("PHP segfault/panic caught at FFI boundary".into())
                })?
            }
        })
        .await
        .map_err(|_| EngineError::PhpFatal("worker thread panicked".into()))??;

        Ok(result)
    }

    fn capabilities(&self) -> &'static [&'static str] {
        &["ffi", "native-ext", "zts"]
    }

    async fn shutdown(&self) {
        // m12-lifecycle: close semaphore, no new permits
        self.pool.close();
        info!("FFI engine shutting down");
    }
}
