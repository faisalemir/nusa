//! Extended edge case tests for nusa-core types.
//!
//! Covers: TenantId security (homoglyphs, null bytes, unicode),
//! Request boundaries, TraceId edge cases, EngineError exhaustive.

use std::path::PathBuf;

use nusa_core::{
    BackpressureGuard, EngineError, RequestContext, ResourceGuard, TenantId, TenantRegistry,
    TraceId, validate_request_size,
};

// ── TenantId Security ──

#[test]
fn tenant_id_empty_string() {
    let id = TenantId::new("");
    assert_eq!(id.to_string(), "");
}

#[test]
fn tenant_id_whitespace_only() {
    let id = TenantId::new("   ");
    assert!(id.to_string().contains("   "));
}

#[test]
fn tenant_id_with_null_bytes() {
    let id = TenantId::new("tenant\0malicious");
    assert!(id.to_string().contains('\0'));
}

#[test]
fn tenant_id_with_control_characters() {
    for i in 0x00..=0x1F {
        let ch = i as u8 as char;
        let input = format!("tenant{}id", ch);
        let id = TenantId::new(&input);
        // Should not panic, should preserve the character
        assert!(id.to_string().contains(ch));
    }
}

#[test]
fn tenant_id_with_del_character() {
    let id = TenantId::new("tenant\x7Fid");
    assert!(id.to_string().contains('\x7F'));
}

#[test]
fn tenant_id_homoglyph_cyrillic_vs_latin() {
    // Latin 'a' vs Cyrillic 'а' (U+0430) - look identical
    let latin = TenantId::new("example");
    let cyrillic = TenantId::new("\u{0435}xample"); // Cyrillic е

    // They should be different strings
    assert_ne!(latin.to_string(), cyrillic.to_string());
}

#[test]
fn tenant_id_with_unicode_emoji() {
    let id = TenantId::new("tenant-🚀-app");
    assert!(id.to_string().contains("🚀"));
}

#[test]
fn tenant_id_with_zero_width_characters() {
    // Zero-width space (U+200B)
    let id = TenantId::new("tenant\u{200B}id");
    assert!(id.to_string().contains('\u{200B}'));
}

#[test]
fn tenant_id_with_zero_width_joiner() {
    // Zero-width joiner (U+200D)
    let id = TenantId::new("tenant\u{200D}id");
    assert!(id.to_string().contains('\u{200D}'));
}

#[test]
fn tenant_id_very_long() {
    let long_name = "a".repeat(10_000);
    let id = TenantId::new(&long_name);
    assert_eq!(id.to_string().len(), 10_000);
}

#[test]
fn tenant_id_single_character() {
    let id = TenantId::new("x");
    assert_eq!(id.to_string(), "x");
}

// ── TenantRegistry Concurrent Operations ──

use nusa_core::TenantConfig;

#[test]
fn tenant_registry_concurrent_register_and_check() {
    let registry = std::sync::Arc::new(std::sync::Mutex::new(TenantRegistry::new()));

    let mut handles = vec![];
    for i in 0..100 {
        let reg = registry.clone();
        handles.push(std::thread::spawn(move || {
            let tenant_id = TenantId::new(format!("tenant-{}", i));
            let config = TenantConfig {
                id: tenant_id.clone(),
                vfs_root: format!("/app/{}", i),
                max_memory_mb: 512,
                max_requests_per_minute: 100,
                enabled: true,
            };
            reg.lock().unwrap().register(config);
            assert!(reg.lock().unwrap().is_enabled(&tenant_id));
        }));
    }

    for h in handles {
        h.join().expect("thread must not panic");
    }
}

#[test]
fn tenant_registry_register_same_tenant_twice() {
    let mut registry = TenantRegistry::new();
    let tenant = TenantId::new("duplicate");

    let config = TenantConfig {
        id: tenant.clone(),
        vfs_root: "/app".into(),
        max_memory_mb: 512,
        max_requests_per_minute: 100,
        enabled: true,
    };

    registry.register(config.clone());
    registry.register(config); // Second register should not panic (overwrite)

    assert!(registry.is_enabled(&tenant));
}

// ── ResourceGuard Boundary Values ──

#[test]
fn resource_guard_max_request_bytes_boundary() {
    let guard = ResourceGuard {
        max_request_bytes: 1024,
        request_timeout_ms: 5000,
        max_concurrent: 10,
    };

    assert!(validate_request_size(Some(1024), guard.max_request_bytes));
    assert!(validate_request_size(Some(1023), guard.max_request_bytes));
    assert!(!validate_request_size(Some(1025), guard.max_request_bytes));
}

#[test]
fn resource_guard_max_request_bytes_zero() {
    let guard = ResourceGuard {
        max_request_bytes: 0,
        request_timeout_ms: 5000,
        max_concurrent: 10,
    };

    assert!(!validate_request_size(Some(1), guard.max_request_bytes));
    assert!(!validate_request_size(Some(0), guard.max_request_bytes));
}

#[test]
fn resource_guard_max_request_bytes_usize_max() {
    let guard = ResourceGuard {
        max_request_bytes: usize::MAX,
        request_timeout_ms: 5000,
        max_concurrent: 10,
    };

    assert!(validate_request_size(
        Some(usize::MAX as u64),
        guard.max_request_bytes
    ));
}

// ── BackpressureGuard Edge Cases ──

#[tokio::test]
async fn backpressure_guard_single_permit() {
    let guard = BackpressureGuard::new(1);

    let permit = guard.try_acquire().await;
    assert!(permit.is_some());

    let second = guard.try_acquire().await;
    assert!(second.is_none());
}

#[tokio::test]
async fn backpressure_guard_zero_permits() {
    let guard = BackpressureGuard::new(0);

    let permit = guard.try_acquire().await;
    assert!(permit.is_none(), "zero-cap guard should never allow");
}

#[tokio::test]
async fn backpressure_guard_large_capacity() {
    let guard = BackpressureGuard::new(1000);

    let mut permits = vec![];
    for _ in 0..1000 {
        let p = guard.try_acquire().await;
        assert!(p.is_some());
        permits.push(p);
    }

    let overflow = guard.try_acquire().await;
    assert!(overflow.is_none(), "should block at 1000");

    drop(permits);
    let after_release = guard.try_acquire().await;
    assert!(after_release.is_some(), "should allow after release");
}

// ── validate_request_size Edge Cases ──

#[test]
fn validate_request_size_u64_max() {
    assert!(!validate_request_size(Some(u64::MAX), 1024));
}

#[test]
fn validate_request_size_usize_max() {
    // On 64-bit: usize::MAX == u64::MAX, so this should fail
    assert!(!validate_request_size(Some(usize::MAX as u64), 1024));
}

#[test]
fn validate_request_size_exact_boundary() {
    assert!(validate_request_size(Some(100), 100));
    assert!(!validate_request_size(Some(101), 100));
}

// ── TraceId Edge Cases ──

#[test]
fn trace_id_display_format() {
    let id = TraceId::new();
    let display = id.to_string();
    // Should be a valid UUID string (36 chars with dashes)
    assert_eq!(display.len(), 36);
    assert_eq!(display.chars().filter(|c| *c == '-').count(), 4);
}

#[test]
fn trace_id_as_uuid_roundtrip() {
    let id = TraceId::new();
    let uuid = id.as_uuid();
    let recovered = TraceId::from_uuid(uuid);
    assert_eq!(id, recovered);
}

#[test]
fn trace_id_default_is_valid() {
    let id = TraceId::default();
    assert!(!id.to_string().is_empty());
}

#[test]
fn trace_id_many_unique() {
    let mut ids = std::collections::HashSet::new();
    for _ in 0..10_000 {
        let id = TraceId::new();
        assert!(ids.insert(id), "each TraceId must be unique");
    }
}

// ── EngineError Exhaustive ──

#[test]
fn engine_error_equality_by_display() {
    let err1 = EngineError::Timeout;
    let err2 = EngineError::Timeout;
    // EngineError doesn't derive Clone or PartialEq, compare via display
    assert_eq!(err1.to_string(), err2.to_string());
    assert_eq!(err1.to_http_status(), err2.to_http_status());
}

#[test]
fn engine_error_display_variants() {
    let errors = [
        EngineError::Timeout,
        EngineError::Sandbox("sandbox error".into()),
        EngineError::PhpFatal("php fatal".into()),
        EngineError::ResourceLimit,
        EngineError::Plugin("plugin error".into()),
        EngineError::IpcProtocol("ipc error".into()),
    ];

    for err in &errors {
        let display = format!("{}", err);
        assert!(
            !display.is_empty(),
            "error display must not be empty: {:?}",
            err
        );
    }
}

#[test]
fn engine_error_sandbox_message_preserved() {
    let msg = "very specific error message";
    let err = EngineError::Sandbox(msg.into());
    assert!(err.to_string().contains(msg));
}

#[test]
fn engine_error_empty_message() {
    let err = EngineError::Sandbox("".into());
    assert_eq!(err.to_string(), "sandbox violation: ");
}

#[test]
fn engine_error_very_long_message() {
    let msg = "x".repeat(100_000);
    let err = EngineError::PhpFatal(msg.clone());
    assert!(err.to_string().contains(&msg));
}

#[test]
fn engine_error_debug_contains_variant_name() {
    let err = EngineError::Timeout;
    let debug = format!("{:?}", err);
    assert!(debug.contains("Timeout"));
}

// ── RequestContext Edge Cases ──

#[test]
fn request_context_with_very_long_vfs_root() {
    let long_path = "/app/".repeat(1000);
    let ctx = RequestContext::new(
        PathBuf::from(&long_path),
        "index.php".into(),
        tokio::time::Instant::now() + std::time::Duration::from_secs(30),
    );
    assert_eq!(ctx.vfs_root().to_string_lossy(), long_path);
}

#[test]
fn request_context_with_very_long_script() {
    let long_script = "a".repeat(10_000);
    let ctx = RequestContext::new(
        "/app".into(),
        PathBuf::from(&long_script),
        tokio::time::Instant::now() + std::time::Duration::from_secs(30),
    );
    assert_eq!(ctx.script_path().to_string_lossy(), long_script);
}

#[test]
fn request_context_with_empty_body() {
    let ctx = RequestContext::new(
        "/app".into(),
        "index.php".into(),
        tokio::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .with_body(bytes::Bytes::new());
    assert!(ctx.body().is_empty());
}

#[test]
fn request_context_with_large_body() {
    let body = bytes::Bytes::from(vec![0u8; 10 * 1024 * 1024]); // 10MB
    let ctx = RequestContext::new(
        "/app".into(),
        "index.php".into(),
        tokio::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .with_body(body.clone());
    assert_eq!(ctx.body().len(), body.len());
}
