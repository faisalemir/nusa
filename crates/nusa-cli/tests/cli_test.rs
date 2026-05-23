//! Integration tests for nusa-cli binary
//!
//! Skills applied:
//! - `domain-cli`: CLI argument parsing
//! - `coding-guidelines`: No get_ prefix, assert! with messages

use clap::{CommandFactory, Parser};
use clap::error::ErrorKind;
use nusa_cli::{Cli, VERSION};

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

#[test]
fn cli_version_constant_matches_workspace() {
    assert!(
        !VERSION.is_empty(),
        "VERSION must be set from CARGO_PKG_VERSION"
    );
    let major_minor_patch = VERSION.split('.').count() >= 2;
    assert!(
        major_minor_patch,
        "VERSION should look like SemVer, got {VERSION}"
    );
}

#[test]
fn cli_version_flag() {
    match Cli::try_parse_from(["nusa", "--version"]) {
        Err(e) => assert_eq!(e.kind(), ErrorKind::DisplayVersion),
        Ok(_) => panic!("--version must not return a parsed Cli"),
    }
}

#[test]
fn cli_version_metadata() {
    let cmd = Cli::command();
    assert_eq!(
        cmd.get_version().map(|s| s.to_string()),
        Some(VERSION.to_string()),
        "clap command version must match workspace VERSION"
    );
}
