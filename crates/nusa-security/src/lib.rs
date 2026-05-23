#![deny(unsafe_code)]
#![warn(clippy::all)]
#![allow(missing_docs)]

//! Security: Landlock and Seccomp-BPF sandboxing for PHP processes.
//!
//! Skills applied:
//! - `domain-cloud-native`: Minimal privilege, syscall whitelist
//! - `m15-anti-pattern`: Security as code, not afterthought

pub mod landlock;
pub mod seccomp;

use std::path::Path;
#[cfg(target_os = "linux")]
use std::sync::Mutex;
#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "linux")]
static LANDLOCK_GUARD: Mutex<()> = Mutex::new(());
#[cfg(target_os = "linux")]
static LANDLOCK_APPLIED: AtomicBool = AtomicBool::new(false);

/// Apply Landlock rules to restrict filesystem access (domain-cloud-native).
/// B1: RO access to code_dir/vfs_root, RW to tmp_dir, deny everything else.
///
/// Landlock is process-wide; a second call returns an error instead of stacking rules.
pub fn apply_landlock(code_dir: &Path, tmp_dir: &Path) -> anyhow::Result<()> {
    apply_landlock_paths(code_dir, tmp_dir, &[])
}

/// Apply Landlock with optional extra RW directories (e.g. Laravel `storage/`).
pub fn apply_landlock_paths(
    code_dir: &Path,
    tmp_dir: &Path,
    extra_rw: &[&Path],
) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    let _guard = LANDLOCK_GUARD
        .lock()
        .map_err(|_| anyhow::anyhow!("Landlock apply lock poisoned"))?;
    #[cfg(target_os = "linux")]
    if LANDLOCK_APPLIED.load(Ordering::Acquire) {
        return Err(anyhow::anyhow!(
            "Landlock rules already applied to this process"
        ));
    }
    landlock::apply_landlock_rules(code_dir, tmp_dir, extra_rw)?;
    #[cfg(target_os = "linux")]
    LANDLOCK_APPLIED.store(true, Ordering::Release);
    Ok(())
}

/// Apply Seccomp-BPF to whitelist syscalls (domain-cloud-native).
/// B2: Whitelist essential syscalls, block dangerous ones.
///
/// Restricts the **current** process. Use only at startup or in a child after `fork`.
pub fn apply_seccomp() -> anyhow::Result<()> {
    seccomp::apply_seccomp_filter()
}

/// Build (but do not install) the Seccomp filter — safe for unit tests in the test runner.
pub fn verify_seccomp_filter() -> anyhow::Result<()> {
    seccomp::verify_seccomp_filter()
}
