//! Initialize [`LaravelHttpRuntime`] from config (IPC or embed backend).

use std::path::PathBuf;
use std::sync::Arc;

use nusa_config::{OctaneBackend, RuntimeConfig};
use nusa_core::LaravelHttpRuntime;
use nusa_engine_embed::FfiWorkerPool;
use nusa_octane_worker::pool::WorkerPool;

/// Shared Laravel runtime handle for gateway + dev reload.
pub type SharedLaravelRuntime = Arc<tokio::sync::Mutex<Option<Box<dyn LaravelHttpRuntime>>>>;

/// Build an Octane/Laravel worker pool when `octane_workers > 0`.
pub async fn init_laravel_runtime(
    cfg: &RuntimeConfig,
) -> anyhow::Result<Option<Box<dyn LaravelHttpRuntime>>> {
    if cfg.octane_workers == 0 {
        return Ok(None);
    }

    let app_root = PathBuf::from(&cfg.code_dir);
    let runtime: Box<dyn LaravelHttpRuntime> = match cfg.octane_backend {
        OctaneBackend::Ipc => {
            let mut pool = WorkerPool::with_standby(
                cfg.octane_workers,
                cfg.octane_standby_workers,
                app_root,
                cfg.octane_max_memory_mb,
                cfg.octane_max_requests,
            );
            pool.initialize().await.map_err(|e| {
                anyhow::anyhow!(
                    "octane_workers={} octane_backend=ipc: pool init failed: {e}",
                    cfg.octane_workers
                )
            })?;
            if !pool.is_ready() {
                anyhow::bail!(
                    "octane_workers={} but no worker has IPC transport (install nusa/octane php-driver)",
                    cfg.octane_workers
                );
            }
            Box::new(pool)
        }
        OctaneBackend::Embed => {
            let php = if cfg.php_binary.is_empty() {
                "php"
            } else {
                cfg.php_binary.as_str()
            };
            let mut pool = FfiWorkerPool::with_standby(
                cfg.octane_workers,
                cfg.octane_standby_workers,
                app_root,
                php.to_string(),
                cfg.octane_max_memory_mb,
                cfg.octane_max_requests,
            );
            pool.initialize().await.map_err(|e| {
                anyhow::anyhow!(
                    "octane_workers={} octane_backend=embed: pool init failed: {e}",
                    cfg.octane_workers
                )
            })?;
            if !pool.is_ready() {
                anyhow::bail!(
                    "octane_workers={} but embed workers failed bootstrap",
                    cfg.octane_workers
                );
            }
            Box::new(pool)
        }
    };

    Ok(Some(runtime))
}
