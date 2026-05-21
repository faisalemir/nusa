//! WASM runtime manager for the PHP WASM engine.
//!
//! Skills applied:
//! - `m03-mutability`: Resource limiter for interior mutability of WASM caps
//! - `m12-lifecycle`: WASM Store lifecycle management
//! - `m05-type-driven`: ResourceLimiter trait implementation (B3)

/// WASM runtime context stored alongside WASI state in the Store.
pub struct WasmRuntimeData {
    pub limits: wasmtime::StoreLimits,
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

    /// Create a stub runtime for testing without a WASM module.
    pub fn stub(memory_mb: u64) -> anyhow::Result<Self> {
        let memory_limit_bytes = memory_mb * 1024 * 1024;
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        let engine = wasmtime::Engine::new(&config)?;

        Ok(Self {
            engine,
            memory_limit_bytes,
        })
    }

    /// Create a WASI store with memory and fuel limits (B3: ResourceLimiter).
    ///
    /// Uses wasmtime's built-in StoreLimits for memory/table caps.
    /// Fuel limits are enforced via Config::consume_fuel(true) set on the engine.
    pub fn create_store(
        &self,
        _work_dir: &std::path::Path,
    ) -> Result<wasmtime::Store<(wasmtime_wasi::WasiCtx, WasmRuntimeData)>, wasmtime::Error> {
        let mut builder = wasmtime_wasi::WasiCtxBuilder::new();
        builder.inherit_stdio();
        let wasi_ctx = builder.build();

        let limits = wasmtime::StoreLimitsBuilder::new()
            .memory_size(self.memory_limit_bytes as usize)
            .build();

        let data = WasmRuntimeData { limits };
        let mut store = wasmtime::Store::new(&self.engine, (wasi_ctx, data));
        store.limiter(|(_, d)| &mut d.limits);
        Ok(store)
    }

    /// Create a WASI store with explicit memory cap.
    pub fn create_store_limited(
        &self,
        _work_dir: &std::path::Path,
        memory_cap: usize,
    ) -> Result<wasmtime::Store<(wasmtime_wasi::WasiCtx, WasmRuntimeData)>, wasmtime::Error> {
        let mut builder = wasmtime_wasi::WasiCtxBuilder::new();
        builder.inherit_stdio();
        let wasi_ctx = builder.build();

        let limits = wasmtime::StoreLimitsBuilder::new()
            .memory_size(memory_cap)
            .build();

        let data = WasmRuntimeData { limits };
        let mut store = wasmtime::Store::new(&self.engine, (wasi_ctx, data));
        store.limiter(|(_, d)| &mut d.limits);
        Ok(store)
    }

    pub fn load_module(&self, bytes: &[u8]) -> Result<wasmtime::Module, wasmtime::Error> {
        wasmtime::Module::new(&self.engine, bytes)
    }

    pub fn memory_limit_bytes(&self) -> u64 {
        self.memory_limit_bytes
    }
}
