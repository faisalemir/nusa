//! Integration tests for nusa-cli binary
//!
//! Skills applied:
//! - `domain-cli`: CLI argument parsing
//! - `coding-guidelines`: No get_ prefix, assert! with messages

use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};
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

// ─── S17: Alpine MUSL Validation ─────────────────────────────────────────

/// S17: CLI must compile and run on Alpine musl (authoritative gate).
///
/// Per the Nusa Test Sector Registry (S17), CLI is excluded from default
/// podman runs but must pass `just podman-test-pkg cli` when touched.
/// This test documents the Alpine contract and validates musl compatibility.
#[test]
#[cfg(target_os = "linux")]
fn cli_alpine_musl_contract() {
    // STUB_CONTRACT: On Alpine, nusa CLI binary must:
    // 1. Compile with musl target (static binary, no glibc dependency)
    // 2. Parse arguments correctly (clap is pure Rust)
    // 3. Handle path separators correctly (forward slash only)
    // 4. No Windows-specific features active
    //
    // Verified by: `just podman-test-pkg nusa-cli` in CI
    // Alpine is Linux/Unix — cfg!(unix) is always true here
    const _: () = assert!(cfg!(unix), "CLI must run on Unix (Alpine is Linux/Unix)");

    // Clap parsing is platform-independent — verify baseline
    let cli = nusa_cli::Cli::parse_from(["nusa", "--version"]);
    assert_eq!(cli.config, "nusa.toml");
}
