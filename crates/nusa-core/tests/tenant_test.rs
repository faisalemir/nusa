//! Integration tests for nusa-core tenant module.
//!
//! Skills applied:
//! - `m05-type-driven`: TenantId as HashMap key
//! - `m09-domain`: Tenant registry isolation
//!
//! Note: Updated to trigger rebuild with new hash to bypass Windows file lock.

use nusa_core::{TenantConfig, TenantId, TenantRegistry};

// ── TenantConfig Construction ──

#[test]
fn tenant_config_serializes_correctly() {
    let config = TenantConfig {
        id: TenantId::new("acme"),
        vfs_root: "/tenants/acme".into(),
        max_memory_mb: 512,
        max_requests_per_minute: 1000,
        enabled: true,
    };

    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("acme"));
    assert!(json.contains("512"));
}

#[test]
fn tenant_config_deserializes_correctly() {
    let json = r#"{
        "id": "test-tenant",
        "vfs_root": "/tenants/test",
        "max_memory_mb": 256,
        "max_requests_per_minute": 500,
        "enabled": true
    }"#;

    let config: TenantConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.max_memory_mb, 256);
    assert!(config.enabled);
}

// ── TenantRegistry Operations ──

#[test]
fn tenant_registry_new_is_empty() {
    let registry = TenantRegistry::new();
    assert!(registry.get(&TenantId::new("nonexistent")).is_none());
}

#[test]
fn tenant_registry_register_and_get() {
    let mut registry = TenantRegistry::new();

    let config = TenantConfig {
        id: TenantId::new("acme"),
        vfs_root: "/tenants/acme".into(),
        max_memory_mb: 512,
        max_requests_per_minute: 1000,
        enabled: true,
    };
    registry.register(config);

    let retrieved = registry.get(&TenantId::new("acme"));
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().max_memory_mb, 512);
}

#[test]
fn tenant_registry_is_enabled_true() {
    let mut registry = TenantRegistry::new();
    registry.register(TenantConfig {
        id: TenantId::new("active"),
        vfs_root: "/tenants/active".into(),
        max_memory_mb: 256,
        max_requests_per_minute: 100,
        enabled: true,
    });

    assert!(registry.is_enabled(&TenantId::new("active")));
}

#[test]
fn tenant_registry_is_enabled_false() {
    let mut registry = TenantRegistry::new();
    registry.register(TenantConfig {
        id: TenantId::new("disabled"),
        vfs_root: "/tenants/disabled".into(),
        max_memory_mb: 256,
        max_requests_per_minute: 100,
        enabled: false,
    });

    assert!(!registry.is_enabled(&TenantId::new("disabled")));
}

#[test]
fn tenant_registry_open_mode_allows_unknown() {
    let registry = TenantRegistry::new();
    assert!(
        registry.is_enabled(&TenantId::new("unknown")),
        "empty registry is open mode"
    );
}

#[test]
fn tenant_registry_is_enabled_missing_tenant() {
    let mut registry = TenantRegistry::new();
    registry.register(TenantConfig {
        id: TenantId::new("known"),
        vfs_root: "/tenants/known".into(),
        max_memory_mb: 256,
        max_requests_per_minute: 100,
        enabled: true,
    });
    assert!(!registry.is_enabled(&TenantId::new("unknown")));
}

#[test]
fn tenant_registry_multiple_tenants() {
    let mut registry = TenantRegistry::new();

    for i in 0..10 {
        registry.register(TenantConfig {
            id: TenantId::new(format!("tenant-{}", i)),
            vfs_root: format!("/tenants/tenant-{}", i),
            max_memory_mb: 256,
            max_requests_per_minute: 1000,
            enabled: i % 2 == 0,
        });
    }

    // Even tenants are enabled, odd are disabled
    for i in 0..10 {
        let id = TenantId::new(format!("tenant-{}", i));
        assert_eq!(registry.is_enabled(&id), i % 2 == 0);
    }
}

#[test]
fn tenant_id_new_unique() {
    let a = TenantId::new("unique-1");
    let b = TenantId::new("unique-2");
    assert_ne!(a, b);
}

#[test]
fn tenant_id_display() {
    let id = TenantId::new("my-tenant");
    assert_eq!(format!("{}", id), "my-tenant");
}

#[test]
fn tenant_id_as_str() {
    let id = TenantId::new("my-tenant");
    assert_eq!(id.as_str(), "my-tenant");
}

// marker
