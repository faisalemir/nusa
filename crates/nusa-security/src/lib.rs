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

/// Apply Landlock rules to restrict filesystem access (domain-cloud-native).
/// B1: RO access to code_dir/vfs_root, RW to tmp_dir, deny everything else.
pub fn apply_landlock(code_dir: &Path, tmp_dir: &Path) -> anyhow::Result<()> {
    landlock::apply_landlock_rules(code_dir, tmp_dir)
}

/// Apply Seccomp-BPF to whitelist syscalls (domain-cloud-native).
/// B2: Whitelist essential syscalls, block dangerous ones.
pub fn apply_seccomp() -> anyhow::Result<()> {
    seccomp::apply_seccomp_filter()
}
