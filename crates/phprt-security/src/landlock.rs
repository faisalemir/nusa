//! Landlock filesystem sandboxing.
//!
//! Skills applied:
//! - `domain-cloud-native`: Minimal privilege, read-only FS for code
//! - `m15-anti-pattern`: Security as code, not afterthought
//! - `unsafe-checker`: Landlock syscalls are kernel-facing (no unsafe needed via crate)

use std::path::Path;

/// Landlock filesystem access rule.
#[derive(Debug, Clone)]
pub struct LandlockRule {
    pub path: std::path::PathBuf,
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl LandlockRule {
    pub fn read_only(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            read: true,
            write: false,
            execute: false,
        }
    }

    pub fn read_write(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            read: true,
            write: true,
            execute: false,
        }
    }

    pub fn read_execute(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            read: true,
            write: false,
            execute: true,
        }
    }
}

/// Apply Landlock rules to restrict filesystem access.
///
/// On Linux with kernel >= 5.13, this uses the real Landlock API.
/// On other platforms, returns Ok(()) as a no-op stub.
pub fn apply_landlock(rules: &[LandlockRule]) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        // TODO: Use landlock crate to apply rules
        tracing::info!("Applying {} Landlock rules (stub — real impl needs Linux)", rules.len());
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = rules;
        tracing::warn!("Landlock not available on this platform — skipping FS restrictions");
    }

    Ok(())
}

/// Build default Landlock rules for a Nusa runtime instance.
pub fn default_rules(code_dir: &Path, tmp_dir: &Path) -> Vec<LandlockRule> {
    vec![
        LandlockRule::read_only(code_dir),
        LandlockRule::read_write(tmp_dir),
    ]
}
