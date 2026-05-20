//! PhpEngine trait for engine-agnostic dispatch.
//!
//! Skills applied:
//! - `m09-domain`: Engine abstraction allows swapping implementations
//! - `m04-zero-cost`: dyn Trait for runtime dispatch
//! - `coding-guidelines`: No get_ prefix on trait methods

use async_trait::async_trait;

use crate::{PhpResponse, RequestContext, Result};

/// Engine Abstraction (m09-domain, m04-zero-cost)
/// Allows swapping FFI/WASM/Child without changing gateway logic.
///
/// # Example
/// ```rust
/// use phprt_core::{PhpEngine, PhpResponse, RequestContext, EngineError};
///
/// // Any engine must implement this trait
/// async fn execute_with_engine(engine: &dyn PhpEngine, ctx: RequestContext) {
///     let result = engine.execute(ctx).await;
///     match result {
///         Ok(response) => assert_eq!(response.status, 200),
///         Err(e) => println!("Engine error: {}", e),
///     }
/// }
/// ```
#[async_trait]
pub trait PhpEngine: Send + Sync + 'static {
    async fn execute(&self, ctx: RequestContext) -> Result<PhpResponse>;
    fn capabilities(&self) -> &'static [&'static str];
    async fn shutdown(&self);
}
