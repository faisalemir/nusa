//! Octane worker pool initialization (fail-closed when `octane_workers > 0`).

use std::path::PathBuf;

use nusa_octane_worker::pool::WorkerPool;

/// Initialize the Octane pool when `octane_workers > 0`.
///
/// Returns `Ok(None)` when workers are disabled. Fails closed if PHP/IPC is unavailable.
pub async fn init_octane_pool(
    octane_workers: usize,
    code_dir: PathBuf,
    octane_max_memory_mb: u64,
    octane_max_requests: u64,
) -> anyhow::Result<Option<WorkerPool>> {
    if octane_workers == 0 {
        return Ok(None);
    }

    let mut pool = WorkerPool::new(
        octane_workers,
        code_dir,
        octane_max_memory_mb,
        octane_max_requests,
    );
    pool.initialize().await.map_err(|e| {
        anyhow::anyhow!("octane_workers={octane_workers} but worker pool failed to initialize: {e}")
    })?;
    if !pool.is_ready() {
        anyhow::bail!(
            "octane_workers={octane_workers} but no worker has IPC transport (check php-driver and PHP)"
        );
    }
    Ok(Some(pool))
}
