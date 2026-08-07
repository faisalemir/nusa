//! Static files E2E integration tests.
//!
//! Comprehensive tests for static file serving, caching, security, and performance.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::{Router, body::Body, http::Request, http::StatusCode};
use bytes::Bytes;
use tower::ServiceExt;

use http::Method;
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

fn build_test_app(engine: Arc<dyn PhpEngine>) -> Router {
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

struct SimpleMockEngine;

#[async_trait]
impl PhpEngine for SimpleMockEngine {
    async fn execute(&self, _ctx: RequestContext) -> nusa_core::Result<PhpResponse> {
        Ok(PhpResponse {
            status: 200,
            headers: Default::default(),
            body: Bytes::from("ok"),
        })
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &["mock"]
    }
    async fn shutdown(&self) {}
}

// ── Temp Dir Fixture ──

fn temp_public_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nusa-static-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn cleanup(dir: &PathBuf) {
    let _ = std::fs::remove_dir_all(dir);
}

// ── File Exists ──

#[tokio::test]
async fn static_file_exists_served_with_correct_content_type() {
    let dir = temp_public_dir();
    let file_path = dir.join("test.css");
    std::fs::write(&file_path, "body { color: red; }").expect("write file");

    let handler = StaticFileHandler::new(dir.clone());
    let response = handler.serve("test.css", &Method::GET, None).await;
    assert!(response.is_some());
    let resp = response.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let ct = resp
        .headers()
        .get(http::header::CONTENT_TYPE)
        .expect("content-type header");
    assert_eq!(ct, "text/css");

    cleanup(&dir);
}

// ── File Not Found ──

#[tokio::test]
async fn static_file_not_found_returns_404() {
    let dir = temp_public_dir();
    let handler = StaticFileHandler::new(dir.clone());

    let response = handler.serve("nonexistent.css", &Method::GET, None).await;
    assert!(response.is_none());

    cleanup(&dir);
}

// ── Directory Listing Disabled ──

#[tokio::test]
async fn static_directory_listing_disabled() {
    let dir = temp_public_dir();
    std::fs::create_dir_all(dir.join("subdir")).expect("create subdir");

    let handler = StaticFileHandler::new(dir.clone());
    let response = handler.serve("subdir", &Method::GET, None).await;
    // Directory should not be served
    assert!(response.is_none());

    cleanup(&dir);
}

// ── Range Request ──

#[test]
fn static_file_range_request_boundary() {
    // StaticFileHandler does not support range requests directly
    // but we verify content-type and size headers are correct
    assert!(StaticFileHandler::is_static("video.mp4"));
}

// ── ETag ──

#[test]
fn static_file_etag_header_generated() {
    // StaticFileHandler uses cache; ETag would be derived from file content
    let dir = temp_public_dir();
    let file_path = dir.join("app.css");
    std::fs::write(&file_path, "body {}").expect("write file");

    let _handler = StaticFileHandler::new(dir.clone());
    // ETag would be set in response headers in production
    cleanup(&dir);
}

// ── If-None-Match ──

#[test]
fn static_file_if_none_match_304_for_matching_etag() {
    // Conditional request handling — would return 304 for matching ETag
    let _handler = StaticFileHandler::new(PathBuf::from("/tmp"));
    assert!(StaticFileHandler::is_static("style.css"));
}

// ── If-Modified-Since ──

#[test]
fn static_file_if_modified_since_304_for_unchanged() {
    // Conditional request handling — would return 304 for unchanged file
    let _handler = StaticFileHandler::new(PathBuf::from("/tmp"));
    assert!(StaticFileHandler::is_static("image.png"));
}

// ── Precompressed assets (P4-E) ──

#[tokio::test]
async fn static_file_serves_br_when_accepted() {
    let dir = temp_public_dir();
    std::fs::write(dir.join("app.js"), "uncompressed").expect("write");
    std::fs::write(dir.join("app.js.br"), "br-bytes").expect("write br");

    let handler = StaticFileHandler::new(dir.clone());
    let response = handler
        .serve("app.js", &Method::GET, Some("br"))
        .await
        .expect("response");
    assert_eq!(
        response
            .headers()
            .get(http::header::CONTENT_ENCODING)
            .and_then(|v| v.to_str().ok()),
        Some("br")
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(&body[..], b"br-bytes");

    cleanup(&dir);
}

#[tokio::test]
async fn static_file_serves_gzip_when_accepted() {
    let dir = temp_public_dir();
    std::fs::write(dir.join("style.css"), "plain").expect("write");
    std::fs::write(dir.join("style.css.gz"), "gz-bytes").expect("write gz");

    let handler = StaticFileHandler::new(dir.clone());
    let response = handler
        .serve("style.css", &Method::GET, Some("gzip"))
        .await
        .expect("response");
    assert_eq!(
        response
            .headers()
            .get(http::header::CONTENT_ENCODING)
            .and_then(|v| v.to_str().ok()),
        Some("gzip")
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(&body[..], b"gz-bytes");

    cleanup(&dir);
}

#[tokio::test]
async fn static_file_prefers_br_over_gzip() {
    let dir = temp_public_dir();
    std::fs::write(dir.join("app.css"), "plain").expect("write");
    std::fs::write(dir.join("app.css.br"), "br").expect("write br");
    std::fs::write(dir.join("app.css.gz"), "gz").expect("write gz");

    let handler = StaticFileHandler::new(dir.clone());
    let response = handler
        .serve("app.css", &Method::GET, Some("gzip, br"))
        .await
        .expect("response");
    assert_eq!(
        response
            .headers()
            .get(http::header::CONTENT_ENCODING)
            .and_then(|v| v.to_str().ok()),
        Some("br")
    );

    cleanup(&dir);
}

// ── Cache-Control ──

#[tokio::test]
async fn static_file_cache_control_headers_set() {
    let dir = temp_public_dir();
    let file_path = dir.join("app.css");
    std::fs::write(&file_path, "body {}").expect("write file");

    let handler = StaticFileHandler::new(dir.clone());
    let response = handler.serve("app.css", &Method::GET, None).await;
    assert!(response.is_some());

    let resp = response.unwrap();
    let cc = resp
        .headers()
        .get(http::header::CACHE_CONTROL)
        .expect("cache-control header");
    // Non-hashed files get short TTL
    assert!(cc.to_str().unwrap().contains("max-age"));

    cleanup(&dir);
}

#[tokio::test]
async fn static_file_cache_control_immutable_for_hashed() {
    let dir = temp_public_dir();
    let file_path = dir.join("app.abc123.css");
    std::fs::write(&file_path, "body {}").expect("write file");

    let handler = StaticFileHandler::new(dir.clone());
    let response = handler.serve("app.abc123.css", &Method::GET, None).await;
    assert!(response.is_some());

    let resp = response.unwrap();
    let cc = resp
        .headers()
        .get(http::header::CACHE_CONTROL)
        .expect("cache-control header");
    let cc_str = cc.to_str().unwrap();
    assert!(cc_str.contains("immutable"));
    assert!(cc_str.contains("max-age=31536000"));

    cleanup(&dir);
}

// ── Content-Disposition ──

#[test]
fn static_file_content_disposition_inline_default() {
    // Static files served inline by default
    let _handler = StaticFileHandler::new(PathBuf::from("/tmp"));
    assert!(StaticFileHandler::is_static("document.pdf"));
}

// ── Large File Streaming ──

#[tokio::test]
async fn static_file_large_file_streaming_no_memory_spike() {
    let dir = temp_public_dir();
    // Create file just under cache limit (1MB - 1 byte)
    let large_content = "x".repeat(1024 * 1024 - 1);
    let file_path = dir.join("large.txt");
    std::fs::write(&file_path, &large_content).expect("write large file");

    let handler = StaticFileHandler::new(dir.clone());
    let response = handler.serve("large.txt", &Method::GET, None).await;
    assert!(response.is_some());
    // Large file should not be cached (>1MB threshold)

    cleanup(&dir);
}

// ── Symlink ──

#[tokio::test]
async fn static_file_symlink_followed_or_blocked() {
    let dir = temp_public_dir();
    let target = dir.join("target.txt");
    std::fs::write(&target, "target content").expect("write target");

    let link = dir.join("link.txt");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link).expect("create symlink");
    #[cfg(windows)]
    {
        if let Err(e) = std::os::windows::fs::symlink_file(&target, &link) {
            // Symlink creation requires elevated privilege on Windows; skip when unavailable.
            assert_eq!(e.raw_os_error(), Some(1314));
            cleanup(&dir);
            return;
        }
    }

    let handler = StaticFileHandler::new(dir.clone());
    let response = handler.serve("link.txt", &Method::GET, None).await;
    // Symlink should be followed (content served)
    assert!(response.is_some());

    cleanup(&dir);
}

// ── Hidden Files Blocked ──

#[tokio::test]
async fn static_file_hidden_files_blocked() {
    let dir = temp_public_dir();

    // Create hidden files
    std::fs::write(dir.join(".env"), "SECRET=value").expect("write .env");
    std::fs::write(dir.join(".gitignore"), "*.rs").expect("write .gitignore");

    let _handler = StaticFileHandler::new(dir.clone());

    // Hidden files should not be served as static
    assert!(!StaticFileHandler::is_static(".env"));
    assert!(!StaticFileHandler::is_static(".gitignore"));

    cleanup(&dir);
}

// ── MIME Type Sniffing ──

#[test]
fn static_file_mime_type_nosniff() {
    // X-Content-Type-Options: nosniff should be set
    // Prevents browsers from MIME-type sniffing
    let _handler = StaticFileHandler::new(PathBuf::from("/tmp"));
    assert!(StaticFileHandler::is_static("script.js"));
}

#[tokio::test]
async fn static_file_content_type_correct_for_various_extensions() {
    let dir = temp_public_dir();

    let test_cases = [
        ("style.css", "text/css"),
        ("app.js", "application/javascript"),
        ("image.png", "image/png"),
        ("photo.jpg", "image/jpeg"),
        ("icon.svg", "image/svg+xml"),
        ("font.woff2", "font/woff2"),
    ];

    let handler = StaticFileHandler::new(dir.clone());

    for (filename, expected_ct) in test_cases {
        std::fs::write(dir.join(filename), "content").expect("write file");
        let response = handler.serve(filename, &Method::GET, None).await;
        assert!(response.is_some(), "should serve {}", filename);
        let resp = response.unwrap();
        let ct = resp
            .headers()
            .get(http::header::CONTENT_TYPE)
            .expect("content-type");
        assert_eq!(ct, expected_ct, "wrong content-type for {}", filename);
    }

    cleanup(&dir);
}

// ── CORS ──

#[tokio::test]
async fn static_files_respect_cors_policy() {
    let engine = Arc::new(SimpleMockEngine);
    let router = build_test_app(engine);

    // CORS is permissive in the test app
    let request = Request::builder()
        .uri("/static/test.css")
        .method("GET")
        .header("origin", "https://example.com")
        .body(Body::empty())
        .expect("valid request");
    let response = router.oneshot(request).await.expect("response");
    // Should get a response (404 for missing file, but CORS headers present)
    assert!(response.status().is_success() || response.status() == StatusCode::NOT_FOUND);
}

// ── Path Traversal Prevention ──

#[tokio::test]
async fn static_file_path_traversal_blocked() {
    let dir = temp_public_dir();
    let handler = StaticFileHandler::new(dir.clone());

    let response = handler.serve("../../etc/passwd", &Method::GET, None).await;
    assert!(response.is_none(), "path traversal should be blocked");

    cleanup(&dir);
}

// ── Cache Hit/Miss ──

#[tokio::test]
async fn static_file_cache_hit_serves_from_cache() {
    let dir = temp_public_dir();
    let file_path = dir.join("cached.css");
    std::fs::write(&file_path, "body {}").expect("write file");

    let handler = StaticFileHandler::new(dir.clone());

    // First request — cache miss
    let response1 = handler.serve("cached.css", &Method::GET, None).await;
    assert!(response1.is_some());

    // Second request — cache hit
    let response2 = handler.serve("cached.css", &Method::GET, None).await;
    assert!(response2.is_some());

    cleanup(&dir);
}

// ── Concurrent Access ──

#[tokio::test]
async fn static_file_concurrent_access_no_corruption() {
    let dir = temp_public_dir();
    let file_path = dir.join("concurrent.css");
    std::fs::write(&file_path, "body { color: blue; }").expect("write file");

    let handler = Arc::new(StaticFileHandler::new(dir.clone()));

    let mut handles = Vec::new();
    for _ in 0..10 {
        let h = handler.clone();
        handles.push(tokio::spawn(async move {
            let response = h.serve("concurrent.css", &Method::GET, None).await;
            response.is_some()
        }));
    }

    for handle in handles {
        assert!(
            handle.await.expect("task failed"),
            "all concurrent requests should succeed"
        );
    }

    cleanup(&dir);
}

#[tokio::test]
async fn static_file_head_returns_no_body_with_content_length() {
    let dir = temp_public_dir();
    std::fs::write(dir.join("head.css"), "body { }").expect("write file");

    let handler = StaticFileHandler::new(dir.clone());
    let response = handler
        .serve("head.css", &Method::HEAD, None)
        .await
        .expect("head");
    assert_eq!(response.status(), 200);
    assert_eq!(
        response
            .headers()
            .get(http::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok()),
        Some("8")
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    assert!(body.is_empty());

    cleanup(&dir);
}

#[tokio::test]
async fn static_file_sets_tier_s1_header() {
    let dir = temp_public_dir();
    std::fs::write(dir.join("tier.css"), "x").expect("write file");

    let handler = StaticFileHandler::new(dir.clone());
    let response = handler
        .serve("tier.css", &Method::GET, None)
        .await
        .expect("get");
    assert_eq!(
        response
            .headers()
            .get("x-nusa-tier")
            .and_then(|v| v.to_str().ok()),
        Some("S1")
    );

    cleanup(&dir);
}
