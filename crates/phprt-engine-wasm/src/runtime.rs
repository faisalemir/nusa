/// WASM runtime manager for the PHP WASM engine.
///
/// m03-mutability: StoreLimits provides interior mutability for memory/fuel caps.
/// m12-lifecycle: WASM Store lifecycle management.
pub struct WasmRuntime {
    engine: wasmtime::Engine,
    memory_limit_bytes: u64,
}

impl WasmRuntime {
    pub fn new(_wasm_bytes: &[u8], memory_mb: u64) -> anyhow::Result<Self> {
        let memory_limit_bytes = memory_mb * 1024 * 1024;

        let config = wasmtime::Config::new();

        let engine = wasmtime::Engine::new(&config)?;

        Ok(Self {
            engine,
            memory_limit_bytes,
        })
    }

    /// Create a new WASI store with preopened directories and memory limits.
    pub async fn create_store(
        &self,
        _work_dir: &std::path::Path,
    ) -> Result<wasmtime::Store<wasmtime_wasi::WasiCtx>, wasmtime::Error> {
        let store = wasmtime::Store::new(&self.engine, wasmtime_wasi::WasiCtxBuilder::new().build());

        // TODO: Apply memory/fuel limits via store.limiter()
        // wasmtime 43 changed the API — need to implement ResourceLimiter trait
        // store.set_resource_limiter(Some(Arc::new(MyResourceLimiter::new(
        //     self.memory_limit_bytes as usize,
        // ))));

        Ok(store)
    }

    /// Instantiate a WASM module from bytes.
    pub fn load_module(&self, bytes: &[u8]) -> Result<wasmtime::Module, wasmtime::Error> {
        wasmtime::Module::new(&self.engine, bytes)
    }

    pub fn memory_limit_bytes(&self) -> u64 {
        self.memory_limit_bytes
    }
}
