#![deny(unsafe_code)]
#![warn(clippy::all)]

//! Security: Landlock and Seccomp-BPF sandboxing for PHP processes.
//!
//! Skills applied:
//! - `domain-cloud-native`: Minimal privilege, syscall whitelist
//! - `m15-anti-pattern`: Security as code, not afterthought

pub mod landlock;
pub mod seccomp;

use std::path::Path;
use tracing::info;

/// Apply Landlock rules to restrict filesystem access (domain-cloud-native)
pub fn apply_landlock(_code_dir: &Path, _tmp_dir: &Path) -> anyhow::Result<()> {
    info!("Applying Landlock rules");
    // TODO: Use landlock crate to set RO for code_dir, RW for tmp_dir
    Ok(())
}

/// Apply Seccomp-BPF to whitelist syscalls
pub fn apply_seccomp() -> anyhow::Result<()> {
    info!("Applying Seccomp-BPF rules");
    // TODO: Use seccomp crate to whitelist essential syscalls
    Ok(())
}
