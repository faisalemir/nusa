//! Multi-tenant isolation system for per-tenant VFS and request routing.
//!
//! Skills applied:
//! - `m05-type-driven`: TenantId used as HashMap key for type safety
//! - `m09-domain`: Tenant registry as aggregate root for isolation

#![warn(clippy::all)]

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::TenantId;

/// Tenant configuration for multi-tenant isolation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantConfig {
    pub id: TenantId,
    pub vfs_root: String,
    pub max_memory_mb: u64,
    pub max_requests_per_minute: u64,
    pub enabled: bool,
}

/// Tenant registry — maps tenant ID to configuration.
///
/// m09-domain: Tenant isolation enforced at type level via TenantId.
#[derive(Debug, Default)]
pub struct TenantRegistry {
    tenants: HashMap<TenantId, TenantConfig>,
}

impl TenantRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            tenants: HashMap::new(),
        }
    }

    pub fn register(&mut self, config: TenantConfig) {
        let id = config.id.clone();
        self.tenants.insert(id, config);
    }

    pub fn get(&self, id: &TenantId) -> Option<&TenantConfig> {
        self.tenants.get(id)
    }

    pub fn is_enabled(&self, id: &TenantId) -> bool {
        self.tenants.get(id).is_some_and(|c| c.enabled)
    }
}
