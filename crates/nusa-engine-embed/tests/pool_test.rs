//! Embed pool tests (stdio daemon when PHP + fixture available).

use std::path::PathBuf;

use nusa_engine_embed::FfiWorkerPool;

fn laravel_fixture() -> Option<PathBuf> {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/laravel-minimal");
    if !root.join("vendor/autoload.php").is_file() {
        return None;
    }
    if nusa_engine_embed::paths::resolve_embed_daemon(&root).is_err() {
        return None;
    }
    // Verify PHP version meets fixture requirement (>= 8.5.0)
    let php = php_binary();
    if let Ok(out) = std::process::Command::new(&php)
        .args(["-r", "echo PHP_MAJOR_VERSION * 100 + PHP_MINOR_VERSION;"])
        .output()
        && let Ok(ver_str) = String::from_utf8(out.stdout)
        && let Ok(ver) = ver_str.trim().parse::<i32>()
        && ver < 805
    {
        eprintln!("skip: PHP version {ver_str} < 8.5 (fixture requires >= 8.5)");
        return None;
    }
    Some(root)
}

fn php_binary() -> String {
    std::env::var("PHP_BINARY")
        .ok()
        .or_else(|| {
            // Alpine PHP 8.5 uses php85, 8.4 uses php84
            if std::process::Command::new("php85")
                .arg("--version")
                .output()
                .is_ok()
            {
                Some("php85".to_string())
            } else if std::process::Command::new("php84")
                .arg("--version")
                .output()
                .is_ok()
            {
                Some("php84".to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "php".to_string())
}

#[tokio::test]
async fn embed_pool_not_ready_before_initialize() {
    let pool = FfiWorkerPool::new(1, PathBuf::from("/nonexistent"), "php".into(), 512, 100);
    assert!(!pool.is_ready());
}

#[tokio::test]
async fn embed_pool_ping_when_fixture_present() {
    let Some(root) = laravel_fixture() else {
        eprintln!("skip embed_pool_ping: laravel-minimal vendor missing");
        return;
    };

    // Authoritative gate: `just podman-ci-embed` (Alpine). Host Windows pipe IPC is flaky.
    // These fixture-dependent tests require `composer dump-autoload` to run, which is only
    // guaranteed in the `podman-ci-embed` tier. Skip in regular `podman-ci`.
    if std::env::var("NUSA_CI_EMBED").is_err() {
        eprintln!("skip embed_pool_ping: requires NUSA_CI_EMBED=1 (run podman-ci-embed)");
        return;
    }
    if cfg!(windows) {
        eprintln!("skip embed_pool_ping on Windows (use podman-ci-embed)");
        return;
    }

    let php = php_binary();
    let mut pool = FfiWorkerPool::new(1, root, php, 512, 50);
    tokio::time::timeout(std::time::Duration::from_secs(60), pool.initialize())
        .await
        .expect("initialize embed pool timed out")
        .expect("initialize embed pool");
    assert!(pool.is_ready());

    let res = pool
        .handle_http_request(
            "GET".into(),
            "/nusa-ping".into(),
            Default::default(),
            None,
            30_000,
        )
        .await
        .expect("ping");
    assert_eq!(res.status, 200);
    assert!(
        std::str::from_utf8(&res.body)
            .expect("utf8 body")
            .contains("pong")
    );

    pool.shutdown().await;
}

#[tokio::test]
async fn embed_pool_db_ping_when_fixture_present() {
    let Some(root) = laravel_fixture() else {
        eprintln!("skip embed_pool_db_ping: laravel-minimal vendor missing");
        return;
    };

    if !std::fs::read_dir(root.join("vendor/composer")).is_ok_and(|e| e.count() > 0) {
        eprintln!("skip embed_pool_db_ping: vendor/composer not populated (run podman-ci-embed)");
        return;
    }
    if cfg!(windows) {
        eprintln!("skip embed_pool_db_ping on Windows (use podman-ci-embed)");
        return;
    }

    let php = php_binary();
    let mut pool = FfiWorkerPool::new(1, root, php, 512, 50);
    tokio::time::timeout(std::time::Duration::from_secs(60), pool.initialize())
        .await
        .expect("initialize embed pool timed out")
        .expect("initialize embed pool");
    assert!(pool.is_ready());

    let res = pool
        .handle_http_request(
            "GET".into(),
            "/nusa-db-ping".into(),
            Default::default(),
            None,
            30_000,
        )
        .await
        .expect("db ping");
    assert_eq!(res.status, 200);
    assert!(
        std::str::from_utf8(&res.body)
            .expect("utf8 body")
            .contains("db:1")
    );

    pool.shutdown().await;
}

#[tokio::test]
async fn embed_pool_ping_frame_transport_when_fixture_present() {
    let Some(root) = laravel_fixture() else {
        eprintln!("skip embed_pool_ping_frame: laravel-minimal vendor missing");
        return;
    };

    if !std::fs::read_dir(root.join("vendor/composer")).is_ok_and(|e| e.count() > 0) {
        eprintln!(
            "skip embed_pool_ping_frame: vendor/composer not populated (run podman-ci-embed)"
        );
        return;
    }
    if cfg!(windows) {
        eprintln!("skip embed_pool_ping_frame on Windows (use podman-ci-embed)");
        return;
    }

    let php = php_binary();

    // SAFETY: test runs before other embed tests in same binary may read env; CI uses isolated jobs.
    unsafe {
        std::env::set_var("NUSA_EMBED_TRANSPORT", "frame");
    }

    let mut pool = FfiWorkerPool::new(1, root, php, 512, 50);
    let init = tokio::time::timeout(std::time::Duration::from_secs(60), pool.initialize()).await;
    unsafe {
        std::env::remove_var("NUSA_EMBED_TRANSPORT");
    }

    init.expect("initialize embed pool (frame) timed out")
        .expect("initialize embed pool (frame)");
    assert!(pool.is_ready());

    let res = pool
        .handle_http_request(
            "GET".into(),
            "/nusa-ping".into(),
            Default::default(),
            None,
            30_000,
        )
        .await
        .expect("ping (frame)");
    assert_eq!(res.status, 200);
    assert!(
        std::str::from_utf8(&res.body)
            .expect("utf8 body")
            .contains("pong")
    );

    pool.shutdown().await;
}

#[tokio::test]
async fn embed_pool_async_spike_frame_transport() {
    let Some(root) = laravel_fixture() else {
        eprintln!("skip embed_pool_async_spike: laravel-minimal vendor missing");
        return;
    };

    if !std::fs::read_dir(root.join("vendor/composer")).is_ok_and(|e| e.count() > 0) {
        eprintln!("skip embed_pool_async_spike: vendor/composer not populated");
        return;
    }
    if cfg!(windows) {
        eprintln!("skip embed_pool_async_spike on Windows (use podman-ci-embed)");
        return;
    }

    let php = php_binary();

    unsafe {
        std::env::set_var("NUSA_EMBED_TRANSPORT", "frame");
        std::env::set_var("NUSA_ASYNC_IO", "stub");
    }

    let mut pool = FfiWorkerPool::new(1, root, php, 512, 50);
    let init = tokio::time::timeout(std::time::Duration::from_secs(60), pool.initialize()).await;
    unsafe {
        std::env::remove_var("NUSA_EMBED_TRANSPORT");
        std::env::remove_var("NUSA_ASYNC_IO");
    }

    init.expect("initialize embed pool (async spike) timed out")
        .expect("initialize embed pool (async spike)");
    assert!(pool.is_ready());

    let res = pool
        .handle_http_request(
            "GET".into(),
            "/nusa-async-spike".into(),
            Default::default(),
            None,
            30_000,
        )
        .await
        .expect("async spike");
    assert_eq!(res.status, 200);
    assert!(
        std::str::from_utf8(&res.body)
            .expect("utf8")
            .contains("async:1")
    );

    pool.shutdown().await;
}

#[tokio::test]
async fn embed_pool_async_sql_generalized() {
    let Some(root) = laravel_fixture() else {
        eprintln!("skip embed_pool_async_sql: laravel-minimal vendor missing");
        return;
    };

    if !std::fs::read_dir(root.join("vendor/composer")).is_ok_and(|e| e.count() > 0) {
        eprintln!("skip embed_pool_async_sql: vendor/composer not populated");
        return;
    }
    if cfg!(windows) {
        eprintln!("skip embed_pool_async_sql on Windows (use podman-ci-embed)");
        return;
    }

    let php = php_binary();

    unsafe {
        std::env::set_var("NUSA_EMBED_TRANSPORT", "frame");
        std::env::set_var("NUSA_ASYNC_IO", "stub");
    }

    let mut pool = FfiWorkerPool::new(1, root, php, 512, 50);
    let init = tokio::time::timeout(std::time::Duration::from_secs(60), pool.initialize()).await;
    unsafe {
        std::env::remove_var("NUSA_EMBED_TRANSPORT");
        std::env::remove_var("NUSA_ASYNC_IO");
    }

    init.expect("initialize timed out")
        .expect("initialize embed pool (async sql)");

    let res = pool
        .handle_http_request(
            "GET".into(),
            "/nusa-async-sql?sql=SELECT%202%20AS%20two".into(),
            Default::default(),
            None,
            30_000,
        )
        .await
        .expect("async sql");
    assert_eq!(res.status, 200);
    let body = std::str::from_utf8(&res.body).expect("utf8");
    assert!(
        body.contains("\"two\":2") || body.contains("\"two\": 2"),
        "expected JSON rows with two=2, got: {body}"
    );

    pool.shutdown().await;
}

#[tokio::test]
async fn embed_pool_db_async_proxy() {
    let Some(root) = laravel_fixture() else {
        eprintln!("skip embed_pool_db_async_proxy: laravel-minimal vendor missing");
        return;
    };

    if !std::fs::read_dir(root.join("vendor/composer")).is_ok_and(|e| e.count() > 0) {
        eprintln!("skip embed_pool_db_async_proxy: vendor/composer not populated");
        return;
    }
    if cfg!(windows) {
        eprintln!("skip embed_pool_db_async_proxy on Windows (use podman-ci-embed)");
        return;
    }

    let php = php_binary();

    unsafe {
        std::env::set_var("NUSA_EMBED_TRANSPORT", "frame");
        std::env::set_var("NUSA_ASYNC_IO", "stub");
    }

    let mut pool = FfiWorkerPool::new(1, root, php, 512, 50);
    let init = tokio::time::timeout(std::time::Duration::from_secs(60), pool.initialize()).await;
    unsafe {
        std::env::remove_var("NUSA_EMBED_TRANSPORT");
        std::env::remove_var("NUSA_ASYNC_IO");
    }

    init.expect("initialize timed out")
        .expect("initialize embed pool (db proxy)");

    let res = pool
        .handle_http_request(
            "GET".into(),
            "/nusa-db-async-proxy".into(),
            Default::default(),
            None,
            30_000,
        )
        .await
        .expect("db async proxy");
    assert_eq!(res.status, 200);
    let body = std::str::from_utf8(&res.body).expect("utf8");
    assert!(
        body.contains("proxy:2"),
        "DB::selectOne must use async proxy, got: {body}"
    );

    pool.shutdown().await;
}
