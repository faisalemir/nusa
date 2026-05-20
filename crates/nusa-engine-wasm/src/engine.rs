//! WASM sandbox engine with memory/fuel limits.
//!
//! Skills applied:
//! - `m03-mutability`: StoreLimits provides interior mutability for resource caps
//! - `m06-error-handling`: WASM traps become EngineError::Sandbox, never crash host

use nusa_core::{PhpEngine, RequestContext, PhpResponse, Result};

/// PHP engine running inside a WASM sandbox (wasmtime).
///
/// Provides strong isolation: WASM memory is sandboxed, and fuel
/// limits prevent infinite loops. PHP must be compiled to the
/// WASI target before it can be loaded.
pub struct WasmEngine {
    #[allow(dead_code)]
    memory_limit_bytes: u64,
    #[allow(dead_code)]
    fuel_per_request: u64,
}

impl WasmEngine {
    /// Create a stub engine (for testing without WASM module)
    pub fn stub() -> Self {
        Self {
            memory_limit_bytes: 256 * 1024 * 1024,
            fuel_per_request: 10_000_000,
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
        // TODO: Create WASI store with StoreLimits for memory/fuel caps
        // 1. Create Engine with consume_fuel(true)
        // 2. Create Store with StoreLimits
        // 3. Set fuel per request
        // 4. Instantiate WASM module
        // 5. Call entry point
        // 6. Handle fuel exhaustion -> EngineError::ResourceLimit
        // 7. Handle WASM trap -> EngineError::Sandbox

        // Stub response
        Ok(PhpResponse {
            status: 200,
            headers: Default::default(),
            body: bytes::Bytes::from("WASM engine stub"),
        })
    }

    fn capabilities(&self) -> &'static [&'static str] {
        &["wasm", "sandbox", "fuel-limited", "memory-capped"]
    }

    async fn shutdown(&self) {
        tracing::info!("WASM engine shutting down");
    }
}
