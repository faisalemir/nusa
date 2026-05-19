#![deny(unsafe_code)]
#![warn(clippy::all)]

use std::sync::LazyLock;
use std::time::Duration;

use figment::{Figment, providers::{Toml, Env, Format}};
use serde::Deserialize;
use arc_swap::ArcSwap;
use std::sync::Arc;

#[derive(Deserialize, Clone, Debug)]
pub struct RuntimeConfig {
    pub engine: EngineKind,
    pub max_workers: usize,
    pub timeout_ms: u64,
    pub wasm_memory_mb: u64,
    pub vfs_root: String,
    pub code_dir: String,
    pub tmp_dir: String,
    pub hot_reload: bool,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub enum EngineKind { Ffi, Wasm, Child }

fn default_config() -> RuntimeConfig {
    RuntimeConfig {
        engine: EngineKind::Child,
        max_workers: 4,
        timeout_ms: 30_000,
        wasm_memory_mb: 256,
        vfs_root: "/app/public".into(),
        code_dir: "/app/public".into(),
        tmp_dir: "/tmp/phprt".into(),
        hot_reload: true,
    }
}

static CONFIG: LazyLock<ArcSwap<RuntimeConfig>> = LazyLock::new(|| {
    ArcSwap::from_pointee(default_config())
});

pub fn load(path: &str) -> anyhow::Result<()> {
    let cfg: RuntimeConfig = Figment::new()
        .merge(Toml::file(path))
        .merge(Env::prefixed("PHPRT_"))
        .extract()?;

    if cfg.max_workers == 0 {
        return Err(anyhow::anyhow!("max_workers must be > 0"));
    }

    CONFIG.store(Arc::new(cfg));
    Ok(())
}

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

        use notify::Watcher;

        let mut watcher = match notify::recommended_watcher(|res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                tracing::debug!("config file changed: {:?}", event);
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

        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
}
