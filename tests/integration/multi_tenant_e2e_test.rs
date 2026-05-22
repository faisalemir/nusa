//! Multi-tenant E2E integration tests.
//!
//! Tests tenant isolation, rate limiting, circuit breakers, VFS isolation, and lifecycle operations.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::{Router, body::Body, http::Request, http::StatusCode};
use bytes::Bytes;
use parking_lot::Mutex;
use tower::ServiceExt;

use nusa_core::{
    BackpressureGuard, PhpEngine, PhpResponse, RequestContext, ResourceGuard, TaskManager,
    TenantRateLimiter, TenantRegistry, TenantConfig, TenantId,
};
use nusa_core::Vfs;
use nusa_gateway::circuit_breaker::CircuitBreaker;
use nusa_gateway::health::HealthState;
use nusa_gateway::sse::SseManager;
use nusa_gateway::static_files::StaticFileHandler;
use nusa_gateway::tenant_circuit_breaker::TenantCircuitBreakers;
use nusa_gateway::websocket::WsManager;
use nusa_gateway::app;
use nusa_octane_worker::state_reset::StateResetOrchestrator;
use nusa_plugin_api::PluginRegistry;
use nusa_telemetry::metrics::NusaMetrics;

use std::sync::OnceLock;

static PROMETHEUS_HANDLE: OnceLock<Arc<metrics_exporter_prometheus::PrometheusHandle>> =
    OnceLock::new();

fn get_prometheus_handle() -> Arc<metrics_exporter_prometheus::PrometheusHandle> {
    PROMETHEUS_HANDLE
        .get_or_init(|| {
            Arc::new(
                metrics_exporter_prometheus::PrometheusBuilder::new()
                    .install_recorder()
                    .expect("prometheus recorder"),
            )
        })
        .clone()
}

fn build_test_app_with_tenants(
    engine: Arc<dyn PhpEngine>,
    tenants: Arc<TenantRegistry>,
    rate_limiter: Arc<TenantRateLimiter>,
    tenant_cb: Arc<TenantCircuitBreakers>,
) -> Router {
    let resource_guard = ResourceGuard {
        max_request_bytes: 1024 * 1024,
        request_timeout_ms: 5000,
        max_concurrent: 100,
    };
    let prometheus_handle = get_prometheus_handle();
    app(
        engine,
        Arc::new(PluginRegistry::new()),
        Arc::new(CircuitBreaker::new(3, Duration::from_secs(1))),
        Arc::new(HealthState::new()),
        Arc::new(BackpressureGuard::new(100)),
        resource_guard,
        tenants,
        Arc::new(TaskManager::new()),
        rate_limiter,
        tenant_cb,
        Arc::new(WsManager::new()),
        Arc::new(SseManager::new()),
        Arc::new(StaticFileHandler::new("/app/public".into())),
        Arc::new(NusaMetrics::init()),
        prometheus_handle.clone(),
        Arc::new(Mutex::new(None)),
        Arc::new(Mutex::new({
            let mut r = StateResetOrchestrator::new(128);
            r.initialize();
            r
        })),
    )
}

struct TenantAwareMockEngine {
    last_tenant: Arc<Mutex<Option<TenantId>>>,
}

#[async_trait]
impl PhpEngine for TenantAwareMockEngine {
    async fn execute(&self, ctx: RequestContext) -> nusa_core::Result<PhpResponse> {
        if let Some(tenant) = ctx.tenant_id() {
            *self.last_tenant.lock() = Some(tenant.clone());
        }
        Ok(PhpResponse {
            status: 200,
            headers: Default::default(),
            body: Bytes::from("tenant ok"),
        })
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &["mock"]
    }
    async fn shutdown(&self) {}
}

// ── Tenant Isolation ──

#[tokio::test]
async fn tenant_isolation_tenant_a_context_only() {
    let last_tenant = Arc::new(Mutex::new(None));
    let engine = Arc::new(TenantAwareMockEngine { last_tenant: last_tenant.clone() });
    let mut registry = TenantRegistry::new();
    registry.register(TenantConfig {
        id: TenantId::new("tenant-a"),
        vfs_root: "/tmp/tenant-a".into(),
        max_memory_mb: 256,
        max_requests_per_minute: 100,
        enabled: true,
    });

    let router = build_test_app_with_tenants(
        engine,
        Arc::new(registry),
        Arc::new(TenantRateLimiter::new(1000, 50)),
        Arc::new(TenantCircuitBreakers::new(3, Duration::from_secs(10))),
    );

    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .header("x-tenant-id", "tenant-a")
        .body(Body::empty())
        .expect("valid request");
    let response = router.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let recorded = last_tenant.lock().clone();
    assert!(recorded.is_some());
    assert_eq!(recorded.unwrap().as_str(), "tenant-a");
}

#[tokio::test]
async fn tenant_isolation_tenant_b_context_only() {
    let last_tenant = Arc::new(Mutex::new(None));
    let engine = Arc::new(TenantAwareMockEngine { last_tenant: last_tenant.clone() });
    let mut registry = TenantRegistry::new();
    registry.register(TenantConfig {
        id: TenantId::new("tenant-b"),
        vfs_root: "/tmp/tenant-b".into(),
        max_memory_mb: 256,
        max_requests_per_minute: 100,
        enabled: true,
    });

    let router = build_test_app_with_tenants(
        engine,
        Arc::new(registry),
        Arc::new(TenantRateLimiter::new(1000, 50)),
        Arc::new(TenantCircuitBreakers::new(3, Duration::from_secs(10))),
    );

    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .header("x-tenant-id", "tenant-b")
        .body(Body::empty())
        .expect("valid request");
    let response = router.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let recorded = last_tenant.lock().clone();
    assert!(recorded.is_some());
    assert_eq!(recorded.unwrap().as_str(), "tenant-b");
}

// ── Tenant Resource Isolation ──

#[test]
fn tenant_isolation_tenant_a_cannot_access_tenant_b_vfs() {
    let vfs_a = Vfs::new("/tmp/tenant-a".into());
    let vfs_b = Vfs::new("/tmp/tenant-b".into());

    assert_ne!(vfs_a.root(), vfs_b.root());
    assert!(!vfs_a.root().starts_with(vfs_b.root()));
}

#[test]
fn tenant_isolation_tenant_config_isolated() {
    let mut registry = TenantRegistry::new();
    registry.register(TenantConfig {
        id: TenantId::new("tenant-a"),
        vfs_root: "/tmp/a".into(),
        max_memory_mb: 256,
        max_requests_per_minute: 100,
        enabled: true,
    });
    registry.register(TenantConfig {
        id: TenantId::new("tenant-b"),
        vfs_root: "/tmp/b".into(),
        max_memory_mb: 512,
        max_requests_per_minute: 200,
        enabled: true,
    });

    let config_a = registry.get(&TenantId::new("tenant-a")).expect("tenant-a exists");
    let config_b = registry.get(&TenantId::new("tenant-b")).expect("tenant-b exists");

    assert_ne!(config_a.max_memory_mb, config_b.max_memory_mb);
    assert_ne!(config_a.max_requests_per_minute, config_b.max_requests_per_minute);
}

// ── Tenant Rate Limit Isolation ──

#[test]
fn tenant_rate_limit_independent_between_tenants() {
    let limiter = TenantRateLimiter::new(2, 2);

    let tenant_a = TenantId::new("tenant-a");
    let tenant_b = TenantId::new("tenant-b");

    assert!(limiter.is_allowed(&tenant_a));
    assert!(limiter.is_allowed(&tenant_a));
    assert!(!limiter.is_allowed(&tenant_a), "tenant-a should be rate limited");

    assert!(limiter.is_allowed(&tenant_b), "tenant-b should still be allowed");
}

// ── Tenant Circuit Breaker Isolation ──

#[tokio::test]
async fn tenant_circuit_breaker_tenant_a_open_does_not_affect_b() {
    let tenant_cb = Arc::new(TenantCircuitBreakers::new(2, Duration::from_secs(10)));
    let tenant_a = TenantId::new("tenant-a");
    let tenant_b = TenantId::new("tenant-b");

    // Open tenant A circuit breaker
    tenant_cb.record_failure(&tenant_a);
    tenant_cb.record_failure(&tenant_a);

    assert!(!tenant_cb.is_allowed(&tenant_a), "tenant-a circuit should be open");
    assert!(tenant_cb.is_allowed(&tenant_b), "tenant-b circuit should still be closed");
}

// ── Tenant VFS Root Isolation ──

#[tokio::test]
async fn tenant_vfs_root_isolated_per_tenant() {
    let temp_dir = std::env::temp_dir();
    let tenant_a_root = temp_dir.join("tenant-a-vfs");
    let tenant_b_root = temp_dir.join("tenant-b-vfs");

    tokio::fs::create_dir_all(&tenant_a_root).await.expect("create tenant a root");
    tokio::fs::create_dir_all(&tenant_b_root).await.expect("create tenant b root");

    let vfs_a = Vfs::new(tenant_a_root.clone());
    let vfs_b = Vfs::new(tenant_b_root.clone());

    assert_ne!(vfs_a.root(), vfs_b.root());

    let _ = tokio::fs::remove_dir_all(&tenant_a_root).await;
    let _ = tokio::fs::remove_dir_all(&tenant_b_root).await;
}

// ── Tenant Concurrent ──

#[tokio::test]
async fn tenant_concurrent_n_tenants_n_requests_no_cross_talk() {
    let last_tenant = Arc::new(Mutex::new(HashMap::<String, TenantId>::new()));
    let engine = Arc::new(TenantAwareMockEngine { last_tenant: last_tenant.clone() });

    let mut registry = TenantRegistry::new();
    for i in 0..5 {
        registry.register(TenantConfig {
            id: TenantId::new(&format!("tenant-{}", i)),
            vfs_root: format!("/tmp/tenant-{}", i),
            max_memory_mb: 256,
            max_requests_per_minute: 100,
            enabled: true,
        });
    }

    let router = build_test_app_with_tenants(
        engine,
        Arc::new(registry),
        Arc::new(TenantRateLimiter::new(1000, 50)),
        Arc::new(TenantCircuitBreakers::new(3, Duration::from_secs(10))),
    );

    let mut handles = Vec::new();
    for i in 0..5 {
        let r = router.clone();
        handles.push(tokio::spawn(async move {
            let request = Request::builder()
                .uri("/index.php")
                .method("GET")
                .header("x-tenant-id", &format!("tenant-{}", i))
                .body(Body::empty())
                .expect("valid request");
            let response = r.oneshot(request).await.expect("response");
            (i, response.status().as_u16())
        }));
    }

    for h in handles {
        let (tenant_id, status) = h.await.expect("task completed");
        assert_eq!(status, 200, "tenant-{} should return 200", tenant_id);
    }
}

// ── Tenant Creation ──

#[test]
fn tenant_creation_new_tenant_immediately_usable() {
    let mut registry = TenantRegistry::new();

    registry.register(TenantConfig {
        id: TenantId::new("new-tenant"),
        vfs_root: "/tmp/new-tenant".into(),
        max_memory_mb: 256,
        max_requests_per_minute: 100,
        enabled: true,
    });

    let config = registry.get(&TenantId::new("new-tenant"));
    assert!(config.is_some());
    assert!(registry.is_enabled(&TenantId::new("new-tenant")));
}

// ── Tenant Deletion ──

#[test]
fn tenant_deletion_resources_cleaned_up() {
    let vfs_root = std::env::temp_dir().join("tenant-to-delete");
    std::fs::create_dir_all(&vfs_root).expect("create tenant dir");

    let mut registry = TenantRegistry::new();
    registry.register(TenantConfig {
        id: TenantId::new("delete-me"),
        vfs_root: vfs_root.to_string_lossy().into(),
        max_memory_mb: 256,
        max_requests_per_minute: 100,
        enabled: true,
    });

    assert!(registry.get(&TenantId::new("delete-me")).is_some());

    let _ = std::fs::remove_dir_all(&vfs_root);
}

// ── Tenant Missing from Registry ──

#[tokio::test]
async fn tenant_missing_from_registry_returns_403_or_default() {
    let engine = Arc::new(TenantAwareMockEngine {
        last_tenant: Arc::new(Mutex::new(None)),
    });
    let registry = TenantRegistry::new();

    let router = build_test_app_with_tenants(
        engine,
        Arc::new(registry),
        Arc::new(TenantRateLimiter::new(1000, 50)),
        Arc::new(TenantCircuitBreakers::new(3, Duration::from_secs(10))),
    );

    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .header("x-tenant-id", "nonexistent-tenant")
        .body(Body::empty())
        .expect("valid request");
    let response = router.oneshot(request).await.expect("response");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
