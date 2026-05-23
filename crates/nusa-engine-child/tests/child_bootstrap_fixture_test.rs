//! Normal-mode child bootstrap against php-static-minimal fixture.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use nusa_core::{PhpEngine, RequestContext};
use nusa_engine_child::ChildEngine;
use tokio::time::Duration;

fn fixture_bootstrap() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/php-static-minimal/bootstrap/nusa-child-ipc.php")
}

#[tokio::test]
async fn child_fixture_bootstrap_returns_static_body_for_root() {
    if Command::new("php").arg("--version").output().is_err() {
        eprintln!("STUB_CONTRACT: php not on PATH; run in Alpine podman-ci for child bootstrap coverage");
        return;
    }

    let bootstrap = fixture_bootstrap();
    assert!(
        bootstrap.is_file(),
        "fixture bootstrap must exist at {}",
        bootstrap.display()
    );

    let engine = ChildEngine::new(PathBuf::from("php"), bootstrap);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::HeaderName::from_static("x-request-method"),
        http::HeaderValue::from_static("GET"),
    );
    headers.insert(
        http::HeaderName::from_static("x-request-uri"),
        http::HeaderValue::from_static("/"),
    );

    let ctx = RequestContext::new(
        "/fixture".into(),
        "index.php".into(),
        deadline,
    )
    .with_headers(headers)
    .with_env(Arc::new(std::collections::HashMap::new()));

    let response = engine
        .execute(ctx)
        .await
        .expect("child IPC bootstrap must return a response");
    assert_eq!(response.status, 200);
    let body = String::from_utf8_lossy(&response.body);
    assert!(
        body.contains("nusa-static-ok"),
        "expected static body, got: {body}"
    );
}
