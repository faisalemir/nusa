//! WASM sandbox engine with memory/fuel limits.
//!
//! **Status:** Stub implementation (M3 milestone). Runtime infrastructure exists in `runtime.rs`
//! but is not yet wired into `execute()`. The `runtime.rs` module provides `WasmRuntime` for
//! wasmtime engine creation with `consume_fuel(true)` and `WasmLimits` implementing
//! `wasmtime::ResourceLimiter` for memory/table caps.
//!
//! Skills applied:
//! - `m03-mutability`: StoreLimits provides interior mutability for resource caps
//! - `m06-error-handling`: WASM traps become EngineError::Sandbox, never crash host

use std::path::PathBuf;
use std::sync::Arc;

use nusa_core::{EngineError, PhpEngine, PhpResponse, RequestContext, Result};
use std::sync::Mutex;

use crate::runtime::WasmRuntime;

/// PHP engine running inside a WASM sandbox (wasmtime).
///
/// Provides strong isolation: WASM memory is sandboxed, and fuel
/// limits prevent infinite loops. PHP must be compiled to the
/// WASI target before it can be loaded.
pub struct WasmEngine {
    runtime: Arc<Mutex<WasmRuntime>>,
    memory_limit_bytes: u64,
    fuel_per_request: u64,
    module: Option<wasmtime::Module>,
}

impl WasmEngine {
    /// Create a new WASM engine from a compiled WASI module.
    pub fn new(wasm_path: PathBuf, memory_mb: u64, fuel: u64) -> Result<Self> {
        let wasm_bytes = std::fs::read(&wasm_path).map_err(|e| EngineError::Sandbox(e.to_string()))?;
        let memory_limit_bytes = memory_mb * 1024 * 1024;

        let runtime = WasmRuntime::new(&wasm_bytes, memory_mb)
            .map_err(|e| EngineError::Sandbox(e.to_string()))?;

        let module = runtime.load_module(&wasm_bytes)
            .map_err(|e| EngineError::Sandbox(e.to_string()))?;

        Ok(Self {
            runtime: Arc::new(Mutex::new(runtime)),
            memory_limit_bytes,
            fuel_per_request: fuel,
            module: Some(module),
        })
    }

    /// Create a stub engine (for testing without WASM module)
    pub fn stub() -> Self {
        Self {
            memory_limit_bytes: 256 * 1024 * 1024,
            fuel_per_request: 10_000_000,
            runtime: Arc::new(Mutex::new(
                WasmRuntime::stub(256).expect("stub runtime should not fail"),
            )),
            module: None,
        }
    }

    /// Return memory limit in bytes.
    pub fn memory_limit_bytes(&self) -> u64 {
        self.memory_limit_bytes
    }

    /// Return fuel limit per request.
    pub fn fuel_per_request(&self) -> u64 {
        self.fuel_per_request
    }
}

#[async_trait::async_trait]
impl PhpEngine for WasmEngine {
    async fn execute(&self, _ctx: RequestContext) -> Result<PhpResponse> {
        let runtime_guard = self.runtime.lock()
            .map_err(|e| EngineError::Sandbox(format!("runtime lock poisoned: {e}")))?;

        // Create store with memory/fuel limits (L3: sandbox isolation)
        let mut store = runtime_guard.create_store_limited(
            &std::path::PathBuf::from("/tmp"),
            self.memory_limit_bytes as usize,
        )
        .map_err(|e| EngineError::Sandbox(e.to_string()))?;

        // Set fuel per request
        store.set_fuel(self.fuel_per_request)
            .map_err(|e| EngineError::Sandbox(e.to_string()))?;

        // Get module or return stub
        let Some(ref module) = self.module else {
            // Stub mode: return a placeholder
            return Ok(PhpResponse {
                status: 200,
                headers: Default::default(),
                body: bytes::Bytes::from("WASM engine running (stub module, no WASI binary loaded)"),
            });
        };

        // Instantiate and run
        let instance = wasmtime::Instance::new(&mut store, module, &[])
            .map_err(|e| EngineError::Sandbox(format!("WASM instantiation failed: {e}")))?;

        // Look for exported "_start" or "main" function
        let entry = instance
            .get_func(&mut store, "_start")
            .or_else(|| instance.get_func(&mut store, "main"))
            .ok_or_else(|| EngineError::Sandbox("No entry point (_start/main) found in WASM module".into()))?;

        // Execute with fuel tracking — if fuel runs out, wasmtime traps
        let result = entry.call(&mut store, &[], &mut []);
        drop(runtime_guard);

        match result {
            Ok(()) => {
                // WASM executed successfully; in a real scenario we'd capture WASI stdout
                Ok(PhpResponse {
                    status: 200,
                    headers: Default::default(),
                    body: bytes::Bytes::from("WASM execution completed"),
                })
            }
            Err(e) if e.to_string().contains("all fuel consumed") => {
                tracing::warn!(fuel = self.fuel_per_request, "WASM fuel exhausted");
                Err(EngineError::ResourceLimit)
            }
            Err(e) => {
                tracing::error!(err = %e, "WASM trap");
                Err(EngineError::Sandbox(format!("WASM trap: {e}")))
            }
        }
    }

    fn capabilities(&self) -> &'static [&'static str] {
        &["wasm", "sandbox", "fuel-limited", "memory-capped"]
    }

    async fn shutdown(&self) {
        tracing::info!("WASM engine shutting down");
    }
}
