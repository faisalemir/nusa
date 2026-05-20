#![deny(unsafe_code)]
#![warn(clippy::all)]
// missing_docs enabled as warning; allowing at crate level until full doc pass is complete
#![allow(missing_docs)]

//! Configuration management with hot-reload support.
//!
//! Skills applied:
//! - `m03-mutability`: ArcSwap for atomic config swap without Mutex
//! - `m12-lifecycle`: Load → watch → reload lifecycle
//! - `m07-concurrency`: LazyLock for global config initialization

use std::sync::LazyLock;

use arc_swap::ArcSwap;
use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::Deserialize;
use std::sync::Arc;

/// Runtime configuration for the Nusa PHP runtime.
///
/// Loaded from TOML file with environment variable overrides (`NUSA_` prefix).
#[derive(Deserialize, Clone, Debug)]
pub struct RuntimeConfig {
    /// PHP engine type: FFI, WASM, or Child.
    pub engine: EngineKind,
    /// Maximum number of concurrent PHP workers.
    pub max_workers: usize,
    /// Per-request timeout in milliseconds.
    pub timeout_ms: u64,
    /// WASM sandbox memory limit in megabytes.
    pub wasm_memory_mb: u64,
    /// Root directory for the virtual filesystem.
    pub vfs_root: String,
    /// Directory containing the Laravel application code.
    pub code_dir: String,
    /// Temporary directory for PHP processes.
    pub tmp_dir: String,
    /// Enable hot-reload on config file changes.
    pub hot_reload: bool,
}

/// PHP engine execution kind.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub enum EngineKind {
    /// PHP embedded via FFI (libphp ZTS).
    Ffi,
    /// PHP running inside WASM sandbox (wasmtime).
    Wasm,
    /// PHP running as child processes.
    Child,
}

fn default_config() -> RuntimeConfig {
    RuntimeConfig {
        engine: EngineKind::Child,
        max_workers: 4,
        timeout_ms: 30_000,
        wasm_memory_mb: 256,
        vfs_root: "/app/public".into(),
        code_dir: "/app/public".into(),
        tmp_dir: "/tmp/nusa".into(),
        hot_reload: true,
    }
}

static CONFIG: LazyLock<ArcSwap<RuntimeConfig>> =
    LazyLock::new(|| ArcSwap::from_pointee(default_config()));

/// Load configuration from a TOML file path.
///
/// Merges file config with `NUSA_`-prefixed environment variables.
/// Validates that `max_workers > 0`.
pub fn load(path: &str) -> anyhow::Result<()> {
    let cfg: RuntimeConfig = Figment::new()
        .merge(Toml::file(path))
        .merge(Env::prefixed("nusa_"))
        .extract()?;

    if cfg.max_workers == 0 {
        return Err(anyhow::anyhow!("max_workers must be > 0"));
    }

    CONFIG.store(Arc::new(cfg));
    Ok(())
}

/// Get the current active configuration.
///
/// Returns an `Arc` clone of the currently stored config via `ArcSwap`.
pub fn get() -> Arc<RuntimeConfig> {
    CONFIG.load().clone()
}

/// Start watching config file for changes.
/// Returns a JoinHandle so the caller can cancel on shutdown.
pub fn watch(path: String) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if !get().hot_reload {
            return;
        }

        use notify::{EventKind, Watcher};

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let mut watcher =
            match notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res
                    && matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_))
                {
                    tracing::info!("config file changed, reloading: {:?}", event.paths);
                    let _ = tx.send(());
                }
            }) {
                Ok(w) => w,
                Err(e) => {
                    tracing::error!(err = %e, "failed to create config watcher");
                    return;
                }
            };

        if let Err(e) = watcher.watch(
            std::path::Path::new(&path),
            notify::RecursiveMode::NonRecursive,
        ) {
            tracing::error!(err = %e, "failed to watch config file");
            return;
        }

        // Keep watcher alive while listening for reload signals
        while let Some(()) = rx.recv().await {
            if let Err(e) = load(&path) {
                tracing::error!(err = %e, "failed to reload config");
            } else {
                tracing::info!("config reloaded successfully");
            }
        }
    })
}
