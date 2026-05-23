//! Deploy state persistence for rollback.

use nusa_cli::deploy::{read_deploy_state, write_deploy_state};

#[test]
fn deploy_state_round_trip() {
    let dir = std::env::temp_dir().join(format!("nusa-deploy-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::env::set_current_dir(&dir).expect("chdir");

    write_deploy_state("nusa.toml", "nusa-green.toml").expect("write");
    let state = read_deploy_state().expect("read");
    assert_eq!(state.previous_config, "nusa.toml");
    assert_eq!(state.active_config, "nusa-green.toml");

    let _ = std::fs::remove_dir_all(dir);
}
