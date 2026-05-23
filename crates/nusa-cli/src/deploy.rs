//! Blue-green deploy state persisted between CLI invocations.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const STATE_DIR: &str = ".nusa";
const STATE_FILE: &str = "deploy-state.toml";

/// Last known active/previous config paths for `nusa rollback`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployState {
    pub previous_config: String,
    pub active_config: String,
}

fn state_path() -> PathBuf {
    PathBuf::from(STATE_DIR).join(STATE_FILE)
}

/// Persist config paths after a successful deploy switch.
pub fn write_deploy_state(previous: &str, active: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(STATE_DIR)?;
    let state = DeployState {
        previous_config: previous.to_string(),
        active_config: active.to_string(),
    };
    let content = toml::to_string_pretty(&state)?;
    std::fs::write(state_path(), content)?;
    Ok(())
}

/// Load deploy state written by the last `nusa deploy`.
pub fn read_deploy_state() -> anyhow::Result<DeployState> {
    let path = state_path();
    let content = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("deploy state not found at {}: {e}", path.display()))?;
    Ok(toml::from_str(&content)?)
}
