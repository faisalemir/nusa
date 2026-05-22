use std::path::Path;

#[cfg(target_os = "linux")]
pub fn apply_landlock_rules(code_dir: &Path, tmp_dir: &Path) -> anyhow::Result<()> {
    use landlock::{
        AccessFs, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr, RulesetStatus,
    };

    tracing::info!("Applying Landlock: RO={:?}, RW={:?}", code_dir, tmp_dir);

    let code_fd = PathFd::new(code_dir)?;
    let tmp_fd = PathFd::new(tmp_dir)?;

    let status = Ruleset::default()
        .handle_access(AccessFs::from_read(landlock::ABI::V1))?
        .handle_access(AccessFs::from_write(landlock::ABI::V1))?
        .create()?
        .add_rule(PathBeneath::new(
            code_fd,
            AccessFs::ReadFile | AccessFs::ReadDir,
        ))?
        .add_rule(PathBeneath::new(
            tmp_fd,
            AccessFs::WriteFile
                | AccessFs::ReadFile
                | AccessFs::ReadDir
                | AccessFs::MakeReg
                | AccessFs::RemoveFile,
        ))?
        .restrict_self()?;

    if status.ruleset == RulesetStatus::FullyEnforced {
        tracing::debug!("Landlock ruleset enforced successfully");
    } else {
        tracing::warn!(
            "Landlock ruleset may not be fully enforced: {:?}",
            status
        );
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn apply_landlock_rules(_code_dir: &Path, _tmp_dir: &Path) -> anyhow::Result<()> {
    tracing::debug!("Landlock not available on this platform, skipping");
    Ok(())
}
