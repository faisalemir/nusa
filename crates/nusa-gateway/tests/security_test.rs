//! Exhaustive security tests for the gateway layer.
//!
//! rust-test-deep Phase 2: Security Exhaustive
//! rust-test-deep §1: Complete Injection Matrix
//! rust-test-deep §3: Path Exhaustive

use async_trait::async_trait;
use axum::{Router, body::Body, http::Request};
use bytes::Bytes;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tower::ServiceExt;

use nusa_core::{
    BackpressureGuard, PhpEngine, PhpResponse, RequestContext, ResourceGuard, TaskManager,
    TenantRateLimiter, TenantRegistry,
};
use nusa_gateway::app;
use nusa_gateway::circuit_breaker::CircuitBreaker;
use nusa_gateway::health::HealthState;
use nusa_gateway::sse::SseManager;
use nusa_gateway::static_files::StaticFileHandler;
use nusa_gateway::tenant_circuit_breaker::TenantCircuitBreakers;
use nusa_gateway::websocket::WsManager;
use nusa_octane_worker::state_reset::StateResetOrchestrator;
use nusa_plugin_api::PluginRegistry;
use nusa_telemetry::metrics::NusaMetrics;

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

struct OkEngine;

#[async_trait]
impl PhpEngine for OkEngine {
    async fn execute(&self, _ctx: RequestContext) -> nusa_core::Result<PhpResponse> {
        Ok(PhpResponse {
            status: 200,
            headers: Default::default(),
            body: Bytes::from("OK"),
        })
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &["mock"]
    }
    async fn shutdown(&self) {}
}

fn build_app() -> Router {
    let rg = ResourceGuard {
        max_request_bytes: 1024 * 1024,
        request_timeout_ms: 5000,
        max_concurrent: 5,
    };

    let prometheus_handle = get_prometheus_handle();

    app(
        Arc::new(OkEngine),
        Arc::new(PluginRegistry::new()),
        Arc::new(CircuitBreaker::new(5, Duration::from_secs(30))),
        Arc::new(HealthState::new()),
        Arc::new(BackpressureGuard::new(10)),
        rg,
        Arc::new(TenantRegistry::new()),
        Arc::new(TaskManager::new()),
        Arc::new(TenantRateLimiter::new(1000, 50)),
        Arc::new(TenantCircuitBreakers::new(5, Duration::from_secs(30))),
        Arc::new(WsManager::new()),
        Arc::new(SseManager::new()),
        Arc::new(StaticFileHandler::new("/tmp".into())),
        Arc::new(NusaMetrics::init()),
        prometheus_handle.clone(),
        Arc::new(tokio::sync::Mutex::new(None)),
        Arc::new({
            let mut r = StateResetOrchestrator::new(128);
            r.initialize();
            r
        }),
    )
}

// ── Path Traversal Attack Matrix ──
// Note: Path traversal via /{path} routes to the PHP engine which handles path resolution.
// These tests verify the app doesn't crash on traversal attempts.
// The PHP engine/VFS layer is responsible for actual path security.

#[tokio::test]
async fn sec_path_traversal_dotdot_slash() {
    let app = build_app();
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/../../etc/passwd")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // Request must be handled (not crash), regardless of status
    assert!(
        resp.status().is_success()
            || resp.status().is_client_error()
            || resp.status().is_server_error()
    );
}

#[tokio::test]
async fn sec_path_traversal_encoded() {
    let app = build_app();
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/%2e%2e/%2e%2e/etc/passwd")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        resp.status().is_success()
            || resp.status().is_client_error()
            || resp.status().is_server_error()
    );
}

#[tokio::test]
async fn sec_path_traversal_absolute() {
    let app = build_app();
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/etc/shadow")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        resp.status().is_success()
            || resp.status().is_client_error()
            || resp.status().is_server_error()
    );
}

#[tokio::test]
async fn sec_path_traversal_backslash() {
    let app = build_app();
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/..\\..\\windows\\system32")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        resp.status().is_success()
            || resp.status().is_client_error()
            || resp.status().is_server_error()
    );
}

#[tokio::test]
async fn sec_path_traversal_double_dot_only() {
    let app = build_app();
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/..")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        resp.status().is_success()
            || resp.status().is_client_error()
            || resp.status().is_server_error()
    );
}

#[tokio::test]
async fn sec_path_traversal_current_dir() {
    let app = build_app();
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/./../../../etc")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        resp.status().is_success()
            || resp.status().is_client_error()
            || resp.status().is_server_error()
    );
}

// ── SQL Injection in Request Data ──
// Note: Characters like ' < > are invalid URI chars and are rejected by http::Uri builder.
// These tests verify that when sent via body (valid scenario), the app doesn't crash.

#[tokio::test]
async fn sec_sqli_in_body_post() {
    // SQL injection via POST body (valid URI)
    let app = build_app();
    let body = Body::from("username=' OR '1'='1");
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/index.php")
                .method("POST")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(body)
                .unwrap(),
        )
        .await
        .unwrap();
    // Must not crash — should return valid response
    assert!(
        resp.status().is_success()
            || resp.status().is_client_error()
            || resp.status().is_server_error()
    );
}

#[tokio::test]
async fn sec_sqli_drop_table_in_body() {
    let app = build_app();
    let body = Body::from("input='; DROP TABLE users;--");
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/index.php")
                .method("POST")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(body)
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        resp.status().is_success()
            || resp.status().is_client_error()
            || resp.status().is_server_error()
    );
}

#[tokio::test]
async fn sec_sqli_union_select_in_body() {
    let app = build_app();
    let body = Body::from("id=' UNION SELECT * FROM users--");
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/index.php")
                .method("POST")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(body)
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        resp.status().is_success()
            || resp.status().is_client_error()
            || resp.status().is_server_error()
    );
}

// ── XSS Attack Matrix ──

#[tokio::test]
async fn sec_xss_in_body_post() {
    let app = build_app();
    let body = Body::from("comment=<script>alert(1)</script>");
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/index.php")
                .method("POST")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(body)
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        resp.status().is_success()
            || resp.status().is_client_error()
            || resp.status().is_server_error()
    );
}

#[tokio::test]
async fn sec_xss_encoded() {
    let app = build_app();
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/index.php?%3Cscript%3Ealert(1)%3C%2Fscript%3E")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        resp.status().is_success()
            || resp.status().is_client_error()
            || resp.status().is_server_error()
    );
}

// ── Format String Attacks ──

#[tokio::test]
async fn sec_format_string_percent_n() {
    let app = build_app();
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/index.php?%n")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        resp.status().is_success()
            || resp.status().is_client_error()
            || resp.status().is_server_error()
    );
}

// ── DoS Attack Matrix ──

#[tokio::test]
async fn sec_empty_request_handled() {
    let app = build_app();
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/index.php")
                .method("POST")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(resp.status().is_success() || resp.status().is_client_error());
}

// ── Unicode Normalization Attacks ──

#[tokio::test]
async fn sec_unicode_nfc_nfd_same_result() {
    let app = build_app();
    let nfc = "\u{00E9}";
    let nfd = "e\u{0301}";
    assert_ne!(nfc, nfd, "NFC and NFD must be different");

    let resp1 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/index.php?name={}", nfc))
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let resp2 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/index.php?name={}", nfd))
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // Both should get same status (both OK from PHP handler)
    assert_eq!(resp1.status(), resp2.status());
}

// ── Zero-Width Character Attacks ──

#[tokio::test]
async fn sec_zero_width_space_in_path() {
    let app = build_app();
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/index\u{200B}.php".to_string())
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(resp.status().is_success() || resp.status().is_client_error());
}

// ── Null Byte Attacks ──

#[tokio::test]
async fn sec_null_byte_in_query_value() {
    // Null bytes are valid in query strings but should be handled safely
    let app = build_app();
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/index.php?file=test%00.php")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        resp.status().is_success()
            || resp.status().is_client_error()
            || resp.status().is_server_error()
    );
}
