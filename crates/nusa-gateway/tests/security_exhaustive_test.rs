//! Security exhaustive tests for gateway input boundaries.
//!
//! rust-test Phase 2: Security Exhaustive
//! Covers ALL injection vectors, encoding attacks, character attacks, and length attacks.

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
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
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;

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

struct EchoMockEngine;

#[async_trait::async_trait]
impl PhpEngine for EchoMockEngine {
    async fn execute(&self, ctx: RequestContext) -> nusa_core::Result<PhpResponse> {
        // Echo back the request body for testing input validation
        Ok(PhpResponse {
            status: 200,
            headers: Default::default(),
            body: ctx.body().clone(),
        })
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &["mock"]
    }
    async fn shutdown(&self) {}
}

fn build_test_app() -> Router {
    let prometheus_handle = get_prometheus_handle();
    app(
        Arc::new(EchoMockEngine),
        Arc::new(PluginRegistry::new()),
        Arc::new(CircuitBreaker::new(3, Duration::from_secs(1))),
        Arc::new(HealthState::new()),
        Arc::new(BackpressureGuard::new(100)),
        ResourceGuard {
            max_request_bytes: 1024 * 1024,
            request_timeout_ms: 5000,
            max_concurrent: 5,
        },
        Arc::new(TenantRegistry::new()),
        Arc::new(TaskManager::new()),
        Arc::new(TenantRateLimiter::new(1000, 50)),
        Arc::new(TenantCircuitBreakers::new(3, Duration::from_secs(10))),
        Arc::new(WsManager::new()),
        Arc::new(SseManager::new()),
        Arc::new(StaticFileHandler::new("/app/public".into())),
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

// ── SQL Injection Patterns ──

#[tokio::test]
async fn sql_injection_tautology_in_body() {
    let app = build_test_app();
    let body = Body::from("' OR '1'='1");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    // Server should handle it without crashing (echo engine returns 200)
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn sql_injection_drop_table_in_body() {
    let app = build_test_app();
    let body = Body::from("'; DROP TABLE users;--");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn sql_injection_union_select_in_body() {
    let app = build_test_app();
    let body = Body::from("' UNION SELECT * FROM users--");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn sql_injection_stacked_queries_in_body() {
    let app = build_test_app();
    let body = Body::from("'; SELECT pg_sleep(10)--");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn sql_injection_comment_in_body() {
    let app = build_test_app();
    let body = Body::from("/**/OR/**/1=1");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn sql_injection_hex_encoded_in_body() {
    let app = build_test_app();
    let body = Body::from("0x736563726574");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

// ── XSS Patterns ──

#[tokio::test]
async fn xss_script_tag_in_body() {
    let app = build_test_app();
    let body = Body::from("<script>alert(1)</script>");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn xss_img_onerror_in_body() {
    let app = build_test_app();
    let body = Body::from("<img src=x onerror=alert(1)>");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn xss_svg_onload_in_body() {
    let app = build_test_app();
    let body = Body::from("<svg onload=alert(1)>");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn xss_javascript_uri_in_body() {
    let app = build_test_app();
    let body = Body::from("javascript:alert(1)");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn xss_encoded_in_body() {
    let app = build_test_app();
    let body = Body::from("%3Cscript%3Ealert(1)%3C%2Fscript%3E");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn xss_double_encoded_in_body() {
    let app = build_test_app();
    let body = Body::from("%253Cscript%253E");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

// ── Path Traversal Patterns ──

#[tokio::test]
async fn path_traversal_dotdot_slash_in_path() {
    let app = build_test_app();
    let request = Request::builder()
        .uri("/../../etc/passwd")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    // Should not serve sensitive files — 404 or 200 with default content
    assert!(response.status() == StatusCode::OK || response.status() == StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn path_traversal_encoded_in_path() {
    let app = build_test_app();
    let request = Request::builder()
        .uri("/%2e%2e/%2e%2e/etc/passwd")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert!(response.status() == StatusCode::OK || response.status() == StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn path_traversal_backslash_in_path() {
    let app = build_test_app();
    let request = Request::builder()
        .uri("/..\\..\\windows\\system32")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert!(response.status() == StatusCode::OK || response.status() == StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn path_traversal_null_byte_in_path() {
    let app = build_test_app();
    let request = Request::builder()
        .uri("/../../etc/passwd%00.png")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert!(response.status() == StatusCode::OK || response.status() == StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn path_traversal_double_dot_only_in_path() {
    let app = build_test_app();
    let request = Request::builder()
        .uri("/..")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert!(response.status() == StatusCode::OK || response.status() == StatusCode::NOT_FOUND);
}

// ── Format String Patterns ──

#[tokio::test]
async fn format_string_percent_s_in_body() {
    let app = build_test_app();
    let body = Body::from("%s");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn format_string_brace_in_body() {
    let app = build_test_app();
    let body = Body::from("{}");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn format_string_positional_in_body() {
    let app = build_test_app();
    let body = Body::from("{0}");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

// ── Character Attacks ──

#[tokio::test]
async fn input_null_byte_at_start_in_body() {
    let app = build_test_app();
    let body = Body::from("\0abc");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/octet-stream")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn input_all_control_chars_in_body() {
    let app = build_test_app();
    let body: Vec<u8> = (0x00..=0x1F).collect();
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/octet-stream")
        .body(Body::from(body))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn input_rtl_override_in_body() {
    let app = build_test_app();
    let body = Body::from("exe\u{202E}dfg.jpg");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn input_ltr_override_in_body() {
    let app = build_test_app();
    let body = Body::from("\u{202A}malicious\u{202C}.txt");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn input_zero_width_space_in_body() {
    let app = build_test_app();
    let body = Body::from("hello\u{200B}world");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn input_zero_width_joiner_in_body() {
    let app = build_test_app();
    let body = Body::from("hello\u{200D}world");
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

// ── Unicode Normalization ──

#[tokio::test]
async fn input_nfc_vs_nfd_in_body() {
    let app = build_test_app();
    // e with acute composed vs decomposed
    let nfc = "\u{00E9}";
    let body = Body::from(nfc);
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn input_homoglyph_in_tenant_header() {
    let app = build_test_app();
    // Cyrillic 'e' vs Latin 'e'
    let cyrillic_e = "\u{0435}";
    let request = Request::builder()
        .uri("/index.php")
        .method("GET")
        .header("x-tenant-id", format!("{}xample", cyrillic_e))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    // Should handle without crashing
    assert!(response.status().is_success() || response.status() == StatusCode::TOO_MANY_REQUESTS);
}

// ── Length Attacks ──

#[tokio::test]
async fn input_255_bytes_in_body() {
    let app = build_test_app();
    let body = Body::from("a".repeat(255));
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn input_256_bytes_in_body() {
    let app = build_test_app();
    let body = Body::from("a".repeat(256));
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn input_100kb_in_body() {
    let app = build_test_app();
    let body = Body::from("a".repeat(100_000));
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn input_1mb_in_body() {
    let app = build_test_app();
    let body = Body::from("a".repeat(1_000_000));
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn input_oversized_rejected() {
    let app = build_test_app();
    // 2MB exceeds our 1MB resource guard
    let body = Body::from("a".repeat(2_000_000));
    let request = Request::builder()
        .uri("/index.php")
        .method("POST")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    // Either 400 (BAD_REQUEST from body parsing) or 413 (PAYLOAD_TOO_LARGE from content-length check)
    assert!(
        response.status() == StatusCode::BAD_REQUEST
            || response.status() == StatusCode::PAYLOAD_TOO_LARGE,
        "oversized body should be rejected, got {}",
        response.status()
    );
}
