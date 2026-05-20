//! Integration tests for nusa-cli binary
//!
//! Skills applied:
//! - `domain-cli`: CLI argument parsing
//! - `coding-guidelines`: No get_ prefix, assert! with messages

use clap::Parser;
use nusa_cli::Cli;

#[test]
fn cli_default_config() {
    let cli = Cli::parse_from(["nusa"]);
    assert_eq!(cli.config, "nusa.toml", "default config must be nusa.toml");
}

#[test]
fn cli_long_flag() {
    let cli = Cli::parse_from(["nusa", "--config", "/etc/nusa/custom.toml"]);
    assert_eq!(cli.config, "/etc/nusa/custom.toml");
}

#[test]
fn cli_short_flag() {
    let cli = Cli::parse_from(["nusa", "-c", "custom.toml"]);
    assert_eq!(cli.config, "custom.toml");
}
