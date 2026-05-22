//! Octane pool init fail-closed (sector S02).
//!
//! STUB_CONTRACT: without php-driver, `init_octane_pool` must Err — never start with `None` pool
//! while `octane_workers > 0`. Live path: `just podman-test-laravel`.

use std::path::PathBuf;

use nusa_cli::octane_pool::init_octane_pool;

fn empty_app_root() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nusa_cli_octane_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp app root");
    dir
}

#[tokio::test]
async fn init_octane_pool_zero_workers_returns_none() {
    let root = empty_app_root();
    let pool = init_octane_pool(0, root, 512, 1000)
        .await
        .expect("zero workers must succeed with None");
    assert!(pool.is_none());
}

#[tokio::test]
async fn init_octane_pool_without_php_driver_fails_closed() {
    let root = empty_app_root();
    let msg = match init_octane_pool(1, root, 512, 1000).await {
        Err(e) => e.to_string(),
        Ok(_) => panic!("octane_workers>0 without php-driver must fail closed"),
    };
    assert!(
        msg.contains("octane_workers=1") || msg.contains("initialize") || msg.contains("IPC"),
        "error must be actionable: {msg}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn init_octane_pool_with_fixture_succeeds_when_vendor_ready() {
    let root = nusa_e2e_tests::laravel_fixture_root();
    let Some(root) = root else {
        return;
    };
    let pool = init_octane_pool(1, root, 512, 500)
        .await
        .expect("fixture with PHP must initialize");
    assert!(pool.is_some());
    let mut pool = pool.expect("Some(pool)");
    assert!(pool.is_ready());
    pool.shutdown().await.ok();
}
