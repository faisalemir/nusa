use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::TenantId;

/// Tenant configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantConfig {
    pub id: TenantId,
    pub vfs_root: String,
    pub max_memory_mb: u64,
    pub max_requests_per_minute: u64,
    pub enabled: bool,
}

/// Tenant registry — maps tenant ID to config.
///
/// m09-domain: Tenant isolation enforced at type level via TenantId.
#[derive(Debug, Default)]
pub struct TenantRegistry {
    tenants: HashMap<String, TenantConfig>,
}

impl TenantRegistry {
    pub fn new() -> Self {
        Self {
            tenants: HashMap::new(),
        }
    }

    pub fn register(&mut self, config: TenantConfig) {
        self.tenants.insert(config.id.as_str().to_string(), config);
    }

    pub fn get(&self, id: &TenantId) -> Option<&TenantConfig> {
        self.tenants.get(id.as_str())
    }

    pub fn is_enabled(&self, id: &TenantId) -> bool {
        self.tenants
            .get(id.as_str())
            .map(|c| c.enabled)
            .unwrap_or(false)
    }
}
