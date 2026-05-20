#![deny(unsafe_code)]
#![warn(clippy::all)]

//! CLI argument parsing for the phprt binary.
//!
//! Skills applied:
//! - `domain-cli`: clap derive for argument parsing

use clap::Parser;

#[derive(Parser)]
#[command(name = "phprt", about = "Nusa PHP Runtime")]
pub struct Cli {
    #[arg(short, long, default_value = "config.toml")]
    pub config: String,
}
