//! Integration tests for phprt-cli binary
//!
//! Skills applied:
//! - `domain-cli`: CLI argument parsing
//! - `coding-guidelines`: No get_ prefix, assert! with messages

use clap::Parser;
use phprt_cli::Cli;

#[test]
fn cli_default_config() {
    let cli = Cli::parse_from(["phprt"]);
    assert_eq!(cli.config, "config.toml", "default config must be config.toml");
}

#[test]
fn cli_long_flag() {
    let cli = Cli::parse_from(["phprt", "--config", "/etc/phprt/custom.toml"]);
    assert_eq!(cli.config, "/etc/phprt/custom.toml");
}

#[test]
fn cli_short_flag() {
    let cli = Cli::parse_from(["phprt", "-c", "custom.toml"]);
    assert_eq!(cli.config, "custom.toml");
}
