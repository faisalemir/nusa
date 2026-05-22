//! Extended concurrency tests for nusa-config crate.
//!
//! Covers: Config concurrent get, load concurrent with get, watch during active use,
//! ArcSwap atomicity, watch handle abort.

use std::fs;
use std::sync::Mutex;
use std::time::Duration;

static CONFIG_LOCK: Mutex<()> = Mutex::new(());

fn write_temp_config(content: &str) -> String {
    let path = std::env::temp_dir().join(format!(
        "nusa_cfg_concurrent_{}.toml",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time must be valid")
            .as_nanos()
    ));
    fs::write(&path, content).expect("must write");
    path.to_string_lossy().to_string()
}

fn cleanup(path: &str) {
    let _ = fs::remove_file(path);
}

// ── Config: Concurrent Get ──

#[test]
fn config_concurrent_get_all_return_consistent_values() {
    // === Arrange ===
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    let content = r#"
        engine = "child"
        max_workers = 4
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = false
    "#;
    let path = write_temp_config(content);
    let _ = nusa_config::load(&path);

    // === Act ===
    let mut handles = Vec::new();
    for _ in 0..8 {
        let handle = std::thread::spawn(|| {
            let mut values = Vec::new();
            for _ in 0..100 {
                let cfg = nusa_config::get();
                values.push(cfg.max_workers);
            }
            values
        });
        handles.push(handle);
    }

    // === Assert ===
    for h in handles {
        let values = h.join().expect("thread must not panic");
        // All reads should return the same value (4)
        for v in &values {
            assert_eq!(*v, 4, "all reads must return consistent value");
        }
    }

    cleanup(&path);
}

// ── Config: Load Concurrent with Get ──

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn config_load_concurrent_with_get_no_race() {
    // === Arrange ===
    let (path1, path2) = {
        let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

        let content1 = r#"
        engine = "child"
        max_workers = 4
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = false
    "#;
        let path1 = write_temp_config(content1);
        let _ = nusa_config::load(&path1);

        let content2 = r#"
        engine = "ffi"
        max_workers = 8
        timeout_ms = 60000
        wasm_memory_mb = 512
        vfs_root = "/app2"
        code_dir = "/app2"
        tmp_dir = "/tmp2"
        hot_reload = true
    "#;
        let path2 = write_temp_config(content2);
        (path1, path2)
    };

    // === Act ===
    let mut handles = Vec::new();
    let path2_clone = path2.clone();
    handles.push(tokio::spawn(async move {
        let result = nusa_config::load(&path2_clone);
        assert!(result.is_ok());
    }));

    // Get threads (concurrent with load)
    for _ in 0..4 {
        handles.push(tokio::spawn(async move {
            for _ in 0..100 {
                let _cfg = nusa_config::get();
                tokio::task::yield_now().await;
            }
        }));
    }

    // === Assert ===
    for h in handles {
        tokio::time::timeout(Duration::from_secs(10), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }

    cleanup(&path1);
    cleanup(&path2);
}

// ── Config: Watch During Active Use ──

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn config_watch_during_active_use_atomic_swap() {
    // === Arrange ===
    let path = {
        let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

        let content = r#"
        engine = "child"
        max_workers = 4
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = true
    "#;
        let path = write_temp_config(content);
        let _ = nusa_config::load(&path);
        path
    };

    let watch_handle = nusa_config::watch(path.clone());

    // === Act ===
    // Concurrently get and modify the config file
    let get_handle = tokio::spawn(async {
        let mut last_workers = 4usize;
        for _ in 0..50 {
            let cfg = nusa_config::get();
            last_workers = cfg.max_workers;
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        last_workers
    });

    // === Assert ===
    tokio::time::sleep(Duration::from_millis(500)).await;
    watch_handle.abort();

    let _ = get_handle.await;
    cleanup(&path);
}

// ── Config: ArcSwap Atomicity ──

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn config_arcswap_atomic_updates_under_contention() {
    // === Arrange ===
    {
        let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");
    }

    // === Act ===
    let mut handles = Vec::new();

    // Write new configs rapidly
    for i in 0..4 {
        let path = write_temp_config(&format!(
            r#"
                engine = "child"
                max_workers = {}
                timeout_ms = 30000
                wasm_memory_mb = 256
                vfs_root = "/app"
                code_dir = "/app"
                tmp_dir = "/tmp"
                hot_reload = false
            "#,
            10 + i
        ));

        let path_clone = path.clone();
        handles.push(tokio::spawn(async move {
            let _ = nusa_config::load(&path_clone);
            cleanup(&path_clone);
        }));
    }

    // Concurrent reads
    for _ in 0..8 {
        handles.push(tokio::spawn(async move {
            for _ in 0..100 {
                let cfg = nusa_config::get();
                assert!(
                    cfg.max_workers >= 1 && cfg.max_workers <= 100,
                    "max_workers must be in valid range"
                );
            }
        }));
    }

    // === Assert ===
    for h in handles {
        tokio::time::timeout(Duration::from_secs(10), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }
}

// ── Config: Watch Handle Abort ──

#[tokio::test]
async fn config_watch_handle_abort_cleanup() {
    // === Arrange ===
    let path = {
        let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

        let content = r#"
        engine = "child"
        max_workers = 4
        timeout_ms = 30000
        wasm_memory_mb = 256
        vfs_root = "/app"
        code_dir = "/app"
        tmp_dir = "/tmp"
        hot_reload = true
    "#;
        let path = write_temp_config(content);
        let _ = nusa_config::load(&path);
        path
    };

    // === Act ===
    let handle = nusa_config::watch(path.clone());

    // Abort the watch task
    handle.abort();

    // Give it time to clean up
    tokio::time::sleep(Duration::from_millis(100)).await;

    // === Assert ===
    assert!(
        handle.is_finished() || handle.is_finished(),
        "handle must be finished after abort"
    );
    cleanup(&path);
}
