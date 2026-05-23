//! Live Laravel + Octane E2E (P2).
//!
//! STUB_CONTRACT: `#[cfg(not(unix))]` tests document that real PHP/IPC runs in Alpine `podman-ci` only.
//! On Linux with fixture vendor, tests fail closed if pool is not ready or response body mismatches.

#[cfg(unix)]
use std::collections::HashMap;
#[cfg(unix)]
use std::path::PathBuf;
#[cfg(unix)]
use std::sync::Arc;
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
use http::header::SET_COOKIE;
#[cfg(unix)]
use nusa_core::PhpResponse;
use nusa_e2e_tests::laravel_fixture_root;
#[cfg(unix)]
use nusa_e2e_tests::{fixture_ready, require_laravel_fixture};
#[cfg(unix)]
use nusa_octane_worker::pool::WorkerPool;

/// First `Set-Cookie` value from a PHP response (session tests).
#[cfg(unix)]
fn first_set_cookie_header(response: &PhpResponse) -> Option<String> {
    response
        .headers
        .get_all(SET_COOKIE)
        .iter()
        .next()
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

/// Cookie request header from a `Set-Cookie` response line (`name=value; ...` → `name=value`).
#[cfg(unix)]
fn cookie_header_from_set_cookie(set_cookie: &str) -> String {
    set_cookie
        .split(';')
        .next()
        .unwrap_or(set_cookie)
        .trim()
        .to_string()
}

#[test]
fn laravel_fixture_contract_non_unix() {
    // STUB_CONTRACT: Windows/macOS host runs mock/stub paths; Alpine podman-ci runs laravel_* tests.
    if cfg!(unix) {
        return;
    }
    assert!(
        laravel_fixture_root().is_none(),
        "non-Unix hosts should not require fixture vendor in default dev runs"
    );
}

#[tokio::test]
#[cfg(unix)]
async fn laravel_fixture_vendor_and_bootstrap_present() {
    let root = require_laravel_fixture();
    assert!(
        fixture_ready(&root),
        "fixture must have vendor, bootstrap, php-driver"
    );
}

#[tokio::test]
#[cfg(unix)]
async fn laravel_worker_pool_initializes_with_ipc_transport() {
    let root = require_laravel_fixture();

    let mut pool = WorkerPool::new(1, root, 512, 500);
    pool.initialize()
        .await
        .expect("pool must initialize when PHP worker and Laravel bootstrap exist");
    assert!(
        pool.is_ready(),
        "production pool must have IPC transport after initialize"
    );
    assert_eq!(pool.idle_count(), 1);

    pool.shutdown().await.expect("shutdown");
}

#[tokio::test]
#[cfg(unix)]
async fn laravel_handle_http_request_returns_fixture_body() {
    let root = require_laravel_fixture();

    let mut pool = WorkerPool::new(1, root, 512, 500);
    pool.initialize().await.expect("initialize");
    assert!(pool.is_ready());

    let response = pool
        .handle_http_request("GET".into(), "/".into(), HashMap::new(), None, 30_000)
        .await
        .expect("HTTP dispatch via IPC");

    assert_eq!(response.status, 200);
    let body = String::from_utf8_lossy(&response.body);
    assert!(
        body.contains("nusa-fixture-ok"),
        "expected fixture route body, got: {body}"
    );

    pool.shutdown().await.ok();
}

#[tokio::test]
#[cfg(unix)]
async fn laravel_two_workers_both_serve_requests() {
    let root = require_laravel_fixture();

    let mut pool = WorkerPool::new(2, root, 512, 500);
    pool.initialize().await.expect("initialize");
    assert!(pool.is_ready());
    assert_eq!(pool.idle_count(), 2);

    for _ in 0..2 {
        let res = pool
            .handle_http_request(
                "GET".into(),
                "/nusa-ping".into(),
                HashMap::new(),
                None,
                30_000,
            )
            .await
            .expect("ping route");
        assert_eq!(res.status, 200);
        assert!(String::from_utf8_lossy(&res.body).contains("pong"));
    }

    pool.shutdown().await.ok();
}

/// Gateway integration: Octane pool ready → HTTP via pool path (not mock engine).
#[tokio::test]
#[cfg(unix)]
async fn laravel_gateway_octane_dispatch_integration() {
    use async_trait::async_trait;
    use axum::{body::Body, http::Request};
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tower::ServiceExt;

    struct FailingEngine {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl PhpEngine for FailingEngine {
        async fn execute(&self, _ctx: RequestContext) -> nusa_core::Result<PhpResponse> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(nusa_core::EngineError::PhpFatal(
                "engine must not run when octane pool is ready".into(),
            ))
        }

        fn capabilities(&self) -> &'static [&'static str] {
            &["fail"]
        }

        async fn shutdown(&self) {}
    }

    let root: PathBuf = require_laravel_fixture();
    let mut pool = WorkerPool::new(1, root, 512, 500);
    pool.initialize().await.expect("initialize");
    assert!(pool.is_ready());

    let calls = Arc::new(AtomicUsize::new(0));
    let engine: Arc<dyn PhpEngine> = Arc::new(FailingEngine {
        calls: calls.clone(),
    });

    let health = Arc::new(HealthState::new());
    health.mark_ready();

    let laravel_runtime: Arc<tokio::sync::Mutex<Option<Box<dyn nusa_core::LaravelHttpRuntime>>>> =
        Arc::new(tokio::sync::Mutex::new(Some(
            Box::new(pool) as Box<dyn nusa_core::LaravelHttpRuntime>
        )));

    static PROM: std::sync::OnceLock<Arc<metrics_exporter_prometheus::PrometheusHandle>> =
        std::sync::OnceLock::new();
    let prom = PROM
        .get_or_init(|| {
            Arc::new(
                metrics_exporter_prometheus::PrometheusBuilder::new()
                    .install_recorder()
                    .expect("prometheus"),
            )
        })
        .clone();

    let mut reset = StateResetOrchestrator::new(128);
    reset.initialize();

    let router = app(
        engine,
        Arc::new(PluginRegistry::new()),
        Arc::new(CircuitBreaker::new(10, Duration::from_secs(30))),
        health,
        Arc::new(BackpressureGuard::new(10)),
        ResourceGuard {
            max_request_bytes: 1024 * 1024,
            request_timeout_ms: 30_000,
            max_concurrent: 10,
        },
        Arc::new(TenantRegistry::new()),
        Arc::new(TaskManager::new()),
        Arc::new(TenantRateLimiter::new(1000, 50)),
        Arc::new(TenantCircuitBreakers::new(10, Duration::from_secs(30))),
        Arc::new(WsManager::new()),
        Arc::new(SseManager::new()),
        Arc::new(StaticFileHandler::new("/tmp".into())),
        Arc::new(NusaMetrics::init()),
        prom,
        laravel_runtime.clone(),
        Arc::new(reset),
    );

    let response = router
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "gateway must use Octane pool, not PhpEngine"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(
        String::from_utf8_lossy(&body).contains("nusa-fixture-ok"),
        "response must come from Laravel fixture"
    );

    let mut guard = laravel_runtime.lock().await;
    if let Some(mut p) = guard.take() {
        p.shutdown().await;
    }
}

#[tokio::test]
#[cfg(unix)]
async fn laravel_post_echo_forwards_request_body() {
    let root = require_laravel_fixture();
    let mut pool = WorkerPool::new(1, root, 512, 500);
    pool.initialize().await.expect("initialize");

    let payload = b"tenant=data&x=1";
    let response = pool
        .handle_http_request(
            "POST".into(),
            "/nusa-echo".into(),
            HashMap::new(),
            Some(payload.to_vec()),
            30_000,
        )
        .await
        .expect("POST echo");

    assert_eq!(response.status, 200);
    let body = String::from_utf8_lossy(&response.body);
    assert!(
        body.contains("echo:") && body.contains("tenant=data"),
        "body must echo POST payload, got: {body}"
    );

    pool.shutdown().await.ok();
}

#[tokio::test]
#[cfg(unix)]
async fn laravel_query_string_reaches_php() {
    let root = require_laravel_fixture();
    let mut pool = WorkerPool::new(1, root, 512, 500);
    pool.initialize().await.expect("initialize");

    let response = pool
        .handle_http_request(
            "GET".into(),
            "/nusa-query?q=octane-test".into(),
            HashMap::new(),
            None,
            30_000,
        )
        .await
        .expect("query route");

    assert_eq!(response.status, 200);
    assert!(
        String::from_utf8_lossy(&response.body).contains("q=octane-test"),
        "query string must reach Laravel"
    );

    pool.shutdown().await.ok();
}

/// Documents worker-local static state across sequential requests (not reset to 1 unless recycled).
#[tokio::test]
#[cfg(unix)]
async fn laravel_counter_shows_worker_persistence_between_requests() {
    let root = require_laravel_fixture();
    let mut pool = WorkerPool::new(1, root, 512, 500);
    pool.initialize().await.expect("initialize");

    let first = pool
        .handle_http_request(
            "GET".into(),
            "/nusa-counter".into(),
            HashMap::new(),
            None,
            30_000,
        )
        .await
        .expect("first counter");
    let second = pool
        .handle_http_request(
            "GET".into(),
            "/nusa-counter".into(),
            HashMap::new(),
            None,
            30_000,
        )
        .await
        .expect("second counter");

    assert_eq!(first.status, 200);
    assert_eq!(second.status, 200);
    let b1 = String::from_utf8_lossy(&first.body);
    let b2 = String::from_utf8_lossy(&second.body);
    assert!(b1.contains("count:1"), "first: {b1}");
    assert!(
        b2.contains("count:2"),
        "same worker must increment static counter (state isolation is separate): {b2}"
    );

    pool.shutdown().await.ok();
}

#[tokio::test]
#[cfg(unix)]
async fn laravel_session_cookie_round_trip_via_ipc_headers() {
    let root = require_laravel_fixture();
    let mut pool = WorkerPool::new(1, root, 512, 500);
    pool.initialize().await.expect("initialize");

    let set_res = pool
        .handle_http_request(
            "GET".into(),
            "/nusa-session-set".into(),
            HashMap::new(),
            None,
            30_000,
        )
        .await
        .expect("session set");
    assert_eq!(set_res.status, 200);

    let set_cookie = first_set_cookie_header(&set_res)
        .expect("session set must return Set-Cookie for array session driver");
    let cookie_value = cookie_header_from_set_cookie(&set_cookie);

    let mut headers = HashMap::new();
    headers.insert("Cookie".into(), vec![cookie_value]);

    let get_res = pool
        .handle_http_request(
            "GET".into(),
            "/nusa-session-get".into(),
            headers,
            None,
            30_000,
        )
        .await
        .expect("session get");

    assert_eq!(get_res.status, 200);
    let body = String::from_utf8_lossy(&get_res.body);
    assert!(
        body.contains("session:fixture-session-ok"),
        "session value must round-trip via Cookie header, got: {body}"
    );

    pool.shutdown().await.ok();
}

#[tokio::test]
#[cfg(unix)]
async fn laravel_custom_middleware_runs_on_worker_path() {
    let root = require_laravel_fixture();
    let mut pool = WorkerPool::new(1, root, 512, 500);
    pool.initialize().await.expect("initialize");

    let response = pool
        .handle_http_request(
            "GET".into(),
            "/nusa-middleware".into(),
            HashMap::new(),
            None,
            30_000,
        )
        .await
        .expect("middleware route");

    assert_eq!(response.status, 200);
    assert!(
        String::from_utf8_lossy(&response.body).contains("mw:1"),
        "NusaFixtureProbe middleware must run in worker HTTP stack"
    );

    pool.shutdown().await.ok();
}

/// Embed backend (`octane_backend = embed`) via stdio daemon — run in `just podman-test-laravel-embed`.
#[tokio::test]
#[cfg(unix)]
async fn laravel_embed_pool_ping() {
    if std::env::var("NUSA_OCTANE_BACKEND").ok().as_deref() != Some("embed") {
        eprintln!("skip laravel_embed_pool_ping: set NUSA_OCTANE_BACKEND=embed");
        return;
    }

    let root = require_laravel_fixture();
    let mut pool = nusa_engine_embed::FfiWorkerPool::new(1, root, "php".into(), 512, 50);
    pool.initialize().await.expect("embed initialize");

    let response = pool
        .handle_http_request(
            "GET".into(),
            "/nusa-ping".into(),
            HashMap::new(),
            None,
            30_000,
        )
        .await
        .expect("embed ping");

    assert_eq!(response.status, 200);
    assert!(
        String::from_utf8_lossy(&response.body).contains("pong"),
        "embed worker must serve Laravel /nusa-ping"
    );

    pool.shutdown().await;
}

/// Session cookie round-trip via embed stdio daemon (gate: `just podman-test-laravel-embed`).
#[tokio::test]
#[cfg(unix)]
async fn laravel_embed_session_cookie_round_trip() {
    if std::env::var("NUSA_OCTANE_BACKEND").ok().as_deref() != Some("embed") {
        eprintln!("skip laravel_embed_session: set NUSA_OCTANE_BACKEND=embed");
        return;
    }

    let root = require_laravel_fixture();
    let mut pool = nusa_engine_embed::FfiWorkerPool::new(1, root, "php".into(), 512, 50);
    pool.initialize().await.expect("embed initialize");

    let set_res = pool
        .handle_http_request(
            "GET".into(),
            "/nusa-session-set".into(),
            HashMap::new(),
            None,
            30_000,
        )
        .await
        .expect("embed session set");

    assert_eq!(set_res.status, 200);
    let set_cookie =
        first_set_cookie_header(&set_res).expect("embed session set must return Set-Cookie");
    let cookie_value = cookie_header_from_set_cookie(&set_cookie);

    let mut headers = HashMap::new();
    headers.insert("Cookie".into(), vec![cookie_value]);

    let get_res = pool
        .handle_http_request(
            "GET".into(),
            "/nusa-session-get".into(),
            headers,
            None,
            30_000,
        )
        .await
        .expect("embed session get");

    assert_eq!(get_res.status, 200);
    let body = String::from_utf8_lossy(&get_res.body);
    assert!(
        body.contains("session:fixture-session-ok"),
        "embed session must round-trip via Cookie header, got: {body}"
    );

    pool.shutdown().await;
}

/// Blocking PDO `SELECT 1` via Laravel DB facade (embed) — baseline before PHP async proxy.
#[tokio::test]
#[cfg(unix)]
async fn laravel_embed_db_ping() {
    if std::env::var("NUSA_OCTANE_BACKEND").ok().as_deref() != Some("embed") {
        eprintln!("skip laravel_embed_db_ping: set NUSA_OCTANE_BACKEND=embed");
        return;
    }

    let root = require_laravel_fixture();
    let mut pool = nusa_engine_embed::FfiWorkerPool::new(1, root, "php".into(), 512, 50);
    pool.initialize().await.expect("embed initialize");

    let response = pool
        .handle_http_request(
            "GET".into(),
            "/nusa-db-ping".into(),
            HashMap::new(),
            None,
            30_000,
        )
        .await
        .expect("embed db ping");

    assert_eq!(response.status, 200);
    assert!(
        String::from_utf8_lossy(&response.body).contains("db:1"),
        "embed worker must run Laravel PDO select 1"
    );

    pool.shutdown().await;
}

/// P4-D: PHP embed async `SELECT 1` via NEB1 op 6/7 (`NUSA_EMBED_TRANSPORT=frame`, `NUSA_ASYNC_IO=stub`).
#[tokio::test]
#[cfg(unix)]
async fn laravel_embed_async_spike() {
    if std::env::var("NUSA_OCTANE_BACKEND").ok().as_deref() != Some("embed") {
        eprintln!("skip laravel_embed_async_spike: set NUSA_OCTANE_BACKEND=embed");
        return;
    }

    let root = require_laravel_fixture();
    unsafe {
        std::env::set_var("NUSA_EMBED_TRANSPORT", "frame");
        std::env::set_var("NUSA_ASYNC_IO", "stub");
    }

    let mut pool = nusa_engine_embed::FfiWorkerPool::new(1, root, "php".into(), 512, 50);
    pool.initialize().await.expect("embed initialize");

    unsafe {
        std::env::remove_var("NUSA_EMBED_TRANSPORT");
        std::env::remove_var("NUSA_ASYNC_IO");
    }

    let response = pool
        .handle_http_request(
            "GET".into(),
            "/nusa-async-spike".into(),
            HashMap::new(),
            None,
            30_000,
        )
        .await
        .expect("async spike");

    assert_eq!(response.status, 200);
    assert!(
        String::from_utf8_lossy(&response.body).contains("async:1"),
        "embed async spike must return async:1"
    );

    pool.shutdown().await;
}

/// P4-D: generalized read-only async SQL via embed NEB1.
#[tokio::test]
#[cfg(unix)]
async fn laravel_embed_async_sql() {
    if std::env::var("NUSA_OCTANE_BACKEND").ok().as_deref() != Some("embed") {
        eprintln!("skip laravel_embed_async_sql: set NUSA_OCTANE_BACKEND=embed");
        return;
    }

    let root = require_laravel_fixture();
    unsafe {
        std::env::set_var("NUSA_EMBED_TRANSPORT", "frame");
        std::env::set_var("NUSA_ASYNC_IO", "stub");
    }

    let mut pool = nusa_engine_embed::FfiWorkerPool::new(1, root, "php".into(), 512, 50);
    pool.initialize().await.expect("embed initialize");

    unsafe {
        std::env::remove_var("NUSA_EMBED_TRANSPORT");
        std::env::remove_var("NUSA_ASYNC_IO");
    }

    let response = pool
        .handle_http_request(
            "GET".into(),
            "/nusa-async-sql".into(),
            HashMap::new(),
            None,
            30_000,
        )
        .await
        .expect("async sql default");

    assert_eq!(response.status, 200);
    let body = String::from_utf8_lossy(&response.body);
    assert!(
        body.contains("\"two\":2") || body.contains("\"two\": 2"),
        "default query SELECT 2 AS two, got: {body}"
    );

    pool.shutdown().await;
}

/// P4-D: Laravel `DB::selectOne` via `NusaAsyncSqliteConnection` + embed async I/O.
#[tokio::test]
#[cfg(unix)]
async fn laravel_embed_db_async_proxy() {
    if std::env::var("NUSA_OCTANE_BACKEND").ok().as_deref() != Some("embed") {
        eprintln!("skip laravel_embed_db_async_proxy: set NUSA_OCTANE_BACKEND=embed");
        return;
    }

    let root = require_laravel_fixture();
    unsafe {
        std::env::set_var("NUSA_EMBED_TRANSPORT", "frame");
        std::env::set_var("NUSA_ASYNC_IO", "stub");
    }

    let mut pool = nusa_engine_embed::FfiWorkerPool::new(1, root, "php".into(), 512, 50);
    pool.initialize().await.expect("embed initialize");

    unsafe {
        std::env::remove_var("NUSA_EMBED_TRANSPORT");
        std::env::remove_var("NUSA_ASYNC_IO");
    }

    let response = pool
        .handle_http_request(
            "GET".into(),
            "/nusa-db-async-proxy".into(),
            HashMap::new(),
            None,
            30_000,
        )
        .await
        .expect("db async proxy");

    assert_eq!(response.status, 200);
    assert!(
        String::from_utf8_lossy(&response.body).contains("proxy:2"),
        "DB facade must route through async proxy"
    );

    pool.shutdown().await;
}
