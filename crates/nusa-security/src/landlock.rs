//! Landlock filesystem sandboxing for PHP processes.
//!
//! Skills applied:
//! - `domain-cloud-native`: Minimal privilege principle
//! - `m15-anti-pattern`: Security enforced before server start

use std::path::Path;

/// Apply Landlock rules to restrict filesystem access.
///
/// Rules:
/// - Read-only access to `code_dir`/`vfs_root`
/// - Read-write access to `tmp_dir`
/// - Deny everything else
#[cfg(target_os = "linux")]
pub fn apply_landlock_rules(code_dir: &Path, tmp_dir: &Path) -> anyhow::Result<()> {
    use landlock::{AccessFs, PathBeneath, Ruleset, RulesetAttr, RulesetCreatedAttr};

    tracing::info!("Applying Landlock: RO={:?}, RW={:?}", code_dir, tmp_dir);

    let ruleset = Ruleset::new()
        .handle_access(AccessFs::from_file(landlock::Access::READ))?
        .create()?
        .add_rule(PathBeneath::new(
            code_dir,
            AccessFs::READ_FILE | AccessFs::READ_DIR,
        ))?
        .create()?
        .restrict_self()?;

    let _ = ruleset;

    // Apply WRITE access to tmp_dir using a separate ruleset
    let tmp_ruleset = Ruleset::new()
        .handle_access(AccessFs::from_file(landlock::Access::WRITE))?
        .create()?
        .add_rule(PathBeneath::new(
            tmp_dir,
            AccessFs::WRITE_FILE | AccessFs::READ_FILE | AccessFs::READ_DIR | AccessFs::MAKE_FILE | AccessFs::REMOVE_FILE,
        ))?
        .create()?
        .restrict_self()?;

    let _ = tmp_ruleset;

    Ok(())
}

/// Stub for non-Linux platforms.
#[cfg(not(target_os = "linux"))]
pub fn apply_landlock_rules(_code_dir: &Path, _tmp_dir: &Path) -> anyhow::Result<()> {
    tracing::debug!("Landlock not available on this platform, skipping");
    Ok(())
}
