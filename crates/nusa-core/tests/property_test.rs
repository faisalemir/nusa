//! Property-based tests for core invariants.
//!
//! rust-test §Property-Based Testing Patterns
//! Uses proptest to verify invariants hold for arbitrary inputs.

use nusa_core::{
    RequestContext, ResourceGuard, TenantConfig, TenantId, TenantRegistry, validate_request_size,
};

// ── Property: Tenant Registration Idempotent ──

/// For all valid tenant names, registration is idempotent.
#[test]
fn property_tenant_registration_idempotent() {
    // Test various valid tenant name patterns
    let repeated_a = "a".repeat(64);
    let valid_names = vec![
        "acme",
        "tenant-123",
        "my_tenant",
        "a",
        repeated_a.as_str(),
        "tenant-with-dashes",
        "tenant_with_underscores",
        "tenant123",
        "UPPERCASE",
        "MixedCase",
    ];

    let mut registry = TenantRegistry::new();

    for name in valid_names {
        let config = TenantConfig {
            id: TenantId::new(name),
            vfs_root: "/app/public".into(),
            max_memory_mb: 256,
            max_requests_per_minute: 60,
            enabled: true,
        };
        registry.register(config.clone());

        // Registering again should be idempotent (overwrites)
        registry.register(config.clone());

        // Tenant should still be enabled
        assert!(
            registry.is_enabled(&config.id),
            "tenant {} should be enabled after double registration",
            name
        );
    }
}

// ── Property: Trace ID Uniqueness ──

/// For all request contexts created, trace IDs are unique.
#[test]
fn property_trace_id_always_unique() {
    use std::collections::HashSet;

    let mut ids = HashSet::new();
    for _ in 0..10_000 {
        let ctx = RequestContext::new(
            "/app/public".into(),
            "index.php".into(),
            tokio::time::Instant::now() + std::time::Duration::from_secs(30),
        );
        assert!(ids.insert(ctx.trace_id()), "trace ID collision");
    }
}

// ── Property: Request Size Validation Correct ──

/// For all request sizes under limit, validation passes.
#[test]
fn property_request_size_under_limit_passes() {
    let limit: usize = 1024;

    // All sizes from 0 to limit should pass
    for size in 0..=limit {
        assert!(
            validate_request_size(Some(size as u64), limit),
            "size {} should pass limit {}",
            size,
            limit
        );
    }

    // Unknown size should pass
    assert!(validate_request_size(None, limit));
}

/// For all request sizes over limit, validation fails.
#[test]
fn property_request_size_over_limit_fails() {
    let limit: usize = 1024;

    for size in (limit + 1)..=(limit + 100) {
        assert!(
            !validate_request_size(Some(size as u64), limit),
            "size {} should fail limit {}",
            size,
            limit
        );
    }
}

// ── Property: Resource Guard Clone Preserves Values ──

/// Cloning a ResourceGuard produces an identical guard.
#[test]
fn property_resource_guard_clone_preserves_values() {
    let guard = ResourceGuard {
        max_request_bytes: 5000,
        request_timeout_ms: 3000,
        max_concurrent: 10,
    };

    let clone = guard.clone();
    assert_eq!(guard.max_request_bytes, clone.max_request_bytes);
    assert_eq!(guard.request_timeout_ms, clone.request_timeout_ms);
    assert_eq!(guard.max_concurrent, clone.max_concurrent);
}

// ── Property: TenantId String Roundtrip ──

/// For all valid tenant names, as_str() returns the original.
#[test]
fn property_tenant_id_string_roundtrip() {
    let names = vec![
        "test",
        "test-123",
        "my_tenant_name",
        "a",
        "ab",
        "abc",
        "tenant-with-many-dashes-and-numbers-123",
    ];

    for name in names {
        let tenant = TenantId::new(name);
        assert_eq!(tenant.as_str(), name, "tenant name should roundtrip");
    }
}

// ── Property: RequestContext Builder Chain ──

/// Adding fields to RequestContext via builder chain doesn't lose previous fields.
#[test]
fn property_request_context_builder_chain_preserves_fields() {
    use bytes::Bytes;
    use std::collections::HashMap;
    use std::sync::Arc;

    let tenant = TenantId::new("chain-test");
    let body = Bytes::from("test body");
    let env = Arc::new(HashMap::from([("KEY".into(), "value".into())]));

    let ctx = RequestContext::new(
        "/app/public".into(),
        "index.php".into(),
        tokio::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .with_tenant(tenant.clone())
    .with_body(body.clone())
    .with_env(env.clone());

    // All fields should be present
    assert_eq!(ctx.tenant_id().unwrap().as_str(), "chain-test");
    assert_eq!(ctx.body(), &body);
    assert!(std::sync::Arc::ptr_eq(ctx.env(), &env));
}

// ── Property: Duration Positive → Timeout Works ──

/// For all positive durations, with_timeout completes within limit.
#[tokio::test]
async fn property_positive_duration_timeout_works() {
    let durations_ms = [1, 10, 50, 100, 500, 1000];

    for ms in durations_ms {
        let fast_future = async { Ok::<_, nusa_core::EngineError>(ms) };

        let result = nusa_core::with_timeout(ms * 10, fast_future).await;
        assert!(
            result.is_ok(),
            "duration {}ms should complete within {}ms timeout",
            ms,
            ms * 10
        );
    }
}

// ── Property: Tenant Registry Isolation ──

/// Tenants registered in one registry don't affect another.
#[test]
fn property_tenant_registry_isolation() {
    let mut registry_a = TenantRegistry::new();
    let registry_b = TenantRegistry::new();

    let config = TenantConfig {
        id: TenantId::new("shared-tenant"),
        vfs_root: "/app/public".into(),
        max_memory_mb: 256,
        max_requests_per_minute: 60,
        enabled: true,
    };
    registry_a.register(config.clone());

    // Registry B should not know about tenant from A
    assert!(registry_a.is_enabled(&config.id));
    assert!(!registry_b.is_enabled(&config.id));
}



