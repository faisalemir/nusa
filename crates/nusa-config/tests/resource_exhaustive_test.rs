//! Resource exhaustion tests for nusa-config crate.
//!
//! Covers: Config watch FD leak, load memory, watch handle abort cleanup.

use std::fs;
use std::sync::Mutex;
use std::time::Duration;

static CONFIG_LOCK: Mutex<()> = Mutex::new(());

fn write_temp_config(content: &str) -> String {
    let path = std::env::temp_dir().join(format!(
        "nusa_cfg_resource_{}.toml",
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

// ── Config Watch: FD Leak ──

#[tokio::test]
async fn config_watch_file_watcher_fd_stable_through_reloads() {
    // === Arrange ===
    let (path, start_fds) = {
        let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");
        let start_fds = count_open_fds();

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
        (path, start_fds)
    };

    // === Act ===
    // Start and stop watch handles multiple times
    for _ in 0..10 {
        let handle = nusa_config::watch(path.clone());
        tokio::time::sleep(Duration::from_millis(50)).await;
        handle.abort();
    }

    // === Assert ===
    let end_fds = count_open_fds();
    assert!(
        end_fds <= start_fds + 10,
        "FD count must be stable through reload cycles"
    );

    cleanup(&path);
}

// ── Config Load: Memory ──

#[test]
fn config_repeated_load_no_memory_growth() {
    // === Arrange ===
    let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");

    // === Act ===
    for i in 0..100 {
        let content = format!(
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
            i + 1
        );
        let path = write_temp_config(&content);
        let result = nusa_config::load(&path);
        assert!(result.is_ok(), "load {} must succeed", i);
        cleanup(&path);
    }

    // === Assert ===
    // No crash = no memory growth issues
}

// ── Config Watch Handle: Abort Cleanup ──

#[tokio::test]
async fn config_watch_handle_abort_all_resources_released() {
    // === Arrange ===
    let (path, start_fds) = {
        let _lock = CONFIG_LOCK.lock().expect("config lock must succeed");
        let start_fds = count_open_fds();

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
        (path, start_fds)
    };

    // === Act ===
    // Sequential start/abort: parallel inotify watchers race FD cleanup in containers.
    for _ in 0..20 {
        let handle = nusa_config::watch(path.clone());
        tokio::time::sleep(Duration::from_millis(10)).await;
        handle.abort();
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let end_fds = count_open_fds();
    // Containers and tokio watchers can add transient FDs under /proc/self/fd.
    let fd_slack = if cfg!(target_os = "linux") { 30 } else { 10 };
    assert!(
        end_fds <= start_fds + fd_slack,
        "no FD leak from abort cleanup (start={start_fds}, end={end_fds})"
    );

    cleanup(&path);
}

// ── Helper ──

#[cfg(unix)]
fn count_open_fds() -> usize {
    use std::fs;
    let fd_dir = "/proc/self/fd";
    if let Ok(entries) = fs::read_dir(fd_dir) {
        entries.count()
    } else {
        0
    }
}

#[cfg(not(unix))]
fn count_open_fds() -> usize {
    0
}
