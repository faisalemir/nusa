//! S11: Config stress decision tests.
//!
//! Covers config loading throughput, decision tables, and hot-reload under stress.
//! Authoritative gate: `just podman-test-pkg nusa-config` (Alpine).

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use nusa_config::{default_config, get, load_sources};

fn temp_toml(name: &str, content: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("nusa_config_stress");
    fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join(name);
    fs::write(&path, content).expect("write temp toml");
    path
}

fn cleanup_temp() {
    let dir = std::env::temp_dir().join("nusa_config_stress");
    let _ = fs::remove_dir_all(&dir);
}

// === Config Load Throughput Under Stress ===

#[test]
fn config_load_throughput_10k_iterations() {
    cleanup_temp();
    let toml = temp_toml(
        "stress.toml",
        r#"
engine = "child"
max_workers = 4
bind = "0.0.0.0:8080"
"#,
    );
    let path = toml.to_str().unwrap();

    let counter = Arc::new(AtomicUsize::new(0));
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let counter = Arc::clone(&counter);
            let path = path.to_string();
            std::thread::spawn(move || {
                for _ in 0..2500 {
                    let result = load_sources(Some(&path));
                    if result.is_ok() {
                        counter.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread panicked");
    }

    let count = counter.load(Ordering::Relaxed);
    assert_eq!(count, 10_000, "all 10K config loads must succeed");

    cleanup_temp();
}

// === Decision Table: Config Validation ===

#[test]
fn config_validation_decision_table() {
    // (toml_content, should_pass)
    let cases = [
        // Valid configs
        (
            r#"engine = "child"
max_workers = 1
bind = "0.0.0.0:8080"
"#,
            true,
        ),
        (
            r#"engine = "ffi"
max_workers = 100
bind = "127.0.0.1:3000"
"#,
            true,
        ),
        (
            r#"engine = "wasm"
max_workers = 1
bind = "[::]:8080"
"#,
            true,
        ),
        // Invalid configs
        (
            r#"engine = "child"
max_workers = 0
bind = "0.0.0.0:8080"
"#,
            false, // max_workers = 0 rejected
        ),
        (
            r#"engine = "child"
max_workers = 4
bind = "not-an-address"
"#,
            false, // invalid bind
        ),
    ];

    cleanup_temp();
    for (i, (content, should_pass)) in cases.iter().enumerate() {
        let toml = temp_toml(&format!("decision_{i}.toml"), content);
        let path = toml.to_str().unwrap();
        let result = load_sources(Some(path));

        if *should_pass {
            assert!(result.is_ok(), "case {i} should pass: {content:?}");
        } else {
            assert!(result.is_err(), "case {i} should fail: {content:?}");
        }
    }
    cleanup_temp();
}

// === Decision Table: Env Override Precedence ===

#[test]
fn config_env_precedence_decision_table() {
    // File value vs env override — env must win
    cleanup_temp();
    let toml = temp_toml(
        "env_decision.toml",
        r#"
engine = "child"
max_workers = 4
bind = "0.0.0.0:8080"
"#,
    );
    let path = toml.to_str().unwrap();

    // Without env override: uses file value
    unsafe { std::env::remove_var("NUSA_MAX_WORKERS") };
    load_sources(Some(path)).expect("load");
    let cfg = get();
    assert_eq!(cfg.max_workers, 4);

    // With env override: env wins
    unsafe { std::env::set_var("NUSA_MAX_WORKERS", "16") };
    load_sources(Some(path)).expect("load");
    let cfg = get();
    assert_eq!(cfg.max_workers, 16);

    // Cleanup
    unsafe { std::env::remove_var("NUSA_MAX_WORKERS") };
    cleanup_temp();
}

// === Load Ramp Test (progressive complexity) ===

#[test]
fn load_ramp_simple_to_complex_config() {
    cleanup_temp();

    let configs = [
        // Minimal
        (
            "minimal.toml",
            r#"
engine = "child"
max_workers = 1
bind = "0.0.0.0:8080"
"#,
        ),
        // Medium (with octane)
        (
            "octane.toml",
            r#"
engine = "child"
max_workers = 4
bind = "0.0.0.0:8080"
octane_workers = 8
octane_backend = "ipc"
"#,
        ),
        // Complex (full config)
        (
            "complex.toml",
            r#"
engine = "child"
max_workers = 8
bind = "0.0.0.0:8080"
timeout_ms = 60000
wasm_memory_mb = 512
code_dir = "/var/www/app"
tmp_dir = "/tmp/nusa"
hot_reload = true
octane_workers = 16
octane_backend = "embed"
octane_max_memory_mb = 1024
octane_max_requests = 5000
octane_standby_workers = 2
static_root = "/var/www/public"
static_cache_max_age_secs = 7200
php_binary = "php85"

[[tenants]]
id = "tenant-a"
vfs_root = "/var/www/tenant-a"
enabled = true
"#,
        ),
    ];

    for (name, content) in configs {
        let toml = temp_toml(name, content);
        let path = toml.to_str().unwrap();
        let result = load_sources(Some(path));
        assert!(result.is_ok(), "{name} should load: {result:?}");
    }

    cleanup_temp();
}

// === Recovery After Stress ===

#[test]
fn config_recovery_after_1000_loads() {
    cleanup_temp();
    let toml = temp_toml(
        "recovery.toml",
        r#"
engine = "child"
max_workers = 4
bind = "0.0.0.0:8080"
"#,
    );
    let path = toml.to_str().unwrap();

    for i in 0..1000 {
        let result = load_sources(Some(path));
        assert!(result.is_ok(), "iteration {i} failed: {result:?}");
    }

    // Final get must still work
    let cfg = get();
    assert_eq!(cfg.max_workers, 4);

    cleanup_temp();
}

// === Hot-Reload Decision Table ===

#[test]
fn hot_reload_decision_table() {
    // hot_reload = true: watch returns handle
    // hot_reload = false: watch exits immediately
    cleanup_temp();

    // hot_reload = true
    let toml = temp_toml(
        "hot_reload_true.toml",
        r#"
engine = "child"
max_workers = 4
bind = "0.0.0.0:8080"
hot_reload = true
"#,
    );
    let path = toml.to_str().unwrap();
    load_sources(Some(path)).expect("load");
    let cfg = get();
    assert!(cfg.hot_reload);

    // hot_reload = false
    let toml2 = temp_toml(
        "hot_reload_false.toml",
        r#"
engine = "child"
max_workers = 4
bind = "0.0.0.0:8080"
hot_reload = false
"#,
    );
    let path2 = toml2.to_str().unwrap();
    load_sources(Some(path2)).expect("load");
    let cfg2 = get();
    assert!(!cfg2.hot_reload);

    cleanup_temp();
}

// === Default Config Values Decision Table ===

#[test]
fn default_config_values_decision_table() {
    let cfg = default_config();

    // Octane defaults
    assert_eq!(cfg.octane_workers, 4);
    assert_eq!(cfg.octane_max_memory_mb, 512);
    assert_eq!(cfg.octane_max_requests, 1000);
    assert_eq!(cfg.octane_standby_workers, 1);

    // Bind defaults
    assert_eq!(cfg.bind, "0.0.0.0:8080");

    // Static cache defaults
    assert_eq!(cfg.static_cache_max_age_secs, 3600);
    assert_eq!(cfg.static_cache_immutable_max_age_secs, 31_536_000);
    assert_eq!(cfg.static_cache_max_entries, 1000);
    assert_eq!(cfg.static_cache_ttl_secs, 300);

    // Engine defaults
    assert!(matches!(cfg.engine, nusa_config::EngineKind::Child));

    // PHP binary default
    assert_eq!(cfg.php_binary, "php");
}
