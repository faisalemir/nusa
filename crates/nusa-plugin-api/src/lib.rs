#![deny(unsafe_code)]
#![warn(clippy::all)]
#![allow(missing_docs)]

//! Plugin system for pre/post execution hooks in the request lifecycle.
//!
//! Skills applied:
//! - `m04-zero-cost`: Trait-based plugin abstraction
//! - `m09-domain`: Plugin registry as domain service
//! - `m07-concurrency`: Clone Arc refs outside lock, drop guard before await

use std::sync::Arc;

use nusa_core::{RequestContext, Result};
use parking_lot::Mutex;

/// Plugin trait for pre/post execution hooks (m04-zero-cost, m09-domain)
#[async_trait::async_trait]
pub trait Plugin: Send + Sync + 'static {
    fn name(&self) -> &'static str;

    async fn pre_exec(&self, _ctx: &mut RequestContext) -> Result<()> {
        Ok(())
    }

    async fn post_exec(&self, _ctx: &RequestContext) -> Result<()> {
        Ok(())
    }
}

/// Plugin registry (m01-ownership: Arc<dyn Plugin> for cross-crate dispatch)
pub struct PluginRegistry {
    plugins: Mutex<Vec<Arc<dyn Plugin>>>,
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: Mutex::new(Vec::new()),
        }
    }

    pub fn register(&self, plugin: Arc<dyn Plugin>) {
        tracing::info!("Registering plugin: {}", plugin.name());
        self.plugins.lock().push(plugin);
    }

    /// Run pre-exec hooks for all plugins.
    /// m07-concurrency: Clone Arc refs outside the lock, drop guard before await.
    pub async fn run_pre_exec(&self, ctx: &mut RequestContext) -> Result<()> {
        let plugins: Vec<_> = self.plugins.lock().iter().cloned().collect();
        for plugin in plugins {
            plugin.pre_exec(ctx).await?;
        }
        Ok(())
    }

    /// Run post-exec hooks for all plugins.
    pub async fn run_post_exec(&self, ctx: &RequestContext) -> Result<()> {
        let plugins: Vec<_> = self.plugins.lock().iter().cloned().collect();
        for plugin in plugins {
            plugin.post_exec(ctx).await?;
        }
        Ok(())
    }
}
