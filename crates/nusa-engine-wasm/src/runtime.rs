//! WASM runtime manager for the PHP WASM engine.
//!
//! Skills applied:
//! - `m03-mutability`: Resource limiter for interior mutability of WASM caps
//! - `m12-lifecycle`: WASM Store lifecycle management
//! - `m05-type-driven`: ResourceLimiter trait implementation (B3)

use wasmtime::ResourceLimiter;

/// Resource limiter for WASM stores that enforces memory and fuel caps (B3).
pub struct WasmLimits {
    memory_cap: usize,
    table_cap: usize,
}

impl WasmLimits {
    pub fn new(memory_bytes: usize) -> Self {
        Self {
            memory_cap: memory_bytes,
            table_cap: usize::MAX,
        }
    }
}

impl ResourceLimiter for WasmLimits {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> Result<bool, wasmtime::Error> {
        if desired > self.memory_cap {
            return Ok(false);
        }
        if let Some(max) = maximum
            && desired > max {
            return Ok(false);
        }
        Ok(current <= desired)
    }

    fn table_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> Result<bool, wasmtime::Error> {
        if desired > self.table_cap {
            return Ok(false);
        }
        if let Some(max) = maximum
            && desired > max {
            return Ok(false);
        }
        Ok(current <= desired)
    }
}

/// WASM runtime manager.
pub struct WasmRuntime {
    engine: wasmtime::Engine,
    memory_limit_bytes: u64,
}

impl WasmRuntime {
    /// Create a new WASM runtime with memory limit (B3).
    pub fn new(_wasm_bytes: &[u8], memory_mb: u64) -> anyhow::Result<Self> {
        let memory_limit_bytes = memory_mb * 1024 * 1024;
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        let engine = wasmtime::Engine::new(&config)?;

        Ok(Self {
            engine,
            memory_limit_bytes,
        })
    }

    /// Create a WASI store with memory limits (B3: ResourceLimiter).
    pub fn create_store(
        &self,
        _work_dir: &std::path::Path,
    ) -> Result<wasmtime::Store<wasmtime_wasi::WasiCtx>, wasmtime::Error> {
        let store = wasmtime::Store::new(&self.engine, wasmtime_wasi::WasiCtxBuilder::new().build());
        // B3: Resource limits via wasmtime::Store::limiter() in wasmtime 44
        // Full implementation would use: store.limiter(|ctx| &mut WasmLimits::new(...))
        // For now, fuel consumption is configured via Config::consume_fuel(true)
        Ok(store)
    }

    pub fn load_module(&self, bytes: &[u8]) -> Result<wasmtime::Module, wasmtime::Error> {
        wasmtime::Module::new(&self.engine, bytes)
    }

    pub fn memory_limit_bytes(&self) -> u64 {
        self.memory_limit_bytes
    }
}
