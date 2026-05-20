//! Nusa CLI binary entrypoint library.
//!
//! Skills applied:
//! - `domain-cli`: clap derive for argument parsing, subcommands
//! - `m12-lifecycle`: explicit init→serve→shutdown phases
//! - `m15-anti-pattern`: Security applied before server start

#![deny(unsafe_code)]
#![warn(clippy::all)]
#![allow(missing_docs)]

pub mod dev;
pub mod test;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "nusa",
    about = "Nusa PHP Runtime — Rust-orchestrated PHP runtime for Laravel"
)]
pub struct Cli {
    /// Path to config file (nusa.toml)
    #[arg(short, long, default_value = "nusa.toml")]
    pub config: String,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the runtime with hot-reload for development
    Dev {
        /// Directories to watch (default: app/, config/, routes/, resources/views/)
        #[arg(long, default_value = "app,config,routes,resources/views,.env")]
        watch: String,

        /// Debounce interval in milliseconds
        #[arg(long, default_value = "200")]
        debounce: u64,

        /// Pretty-print logs (disable JSON formatting)
        #[arg(long)]
        pretty: bool,
    },

    /// Run tests with persistent worker pool
    Test {
        /// Test file or directory
        #[arg(default_value = "tests/")]
        path: String,

        /// Number of test workers
        #[arg(long, default_value = "4")]
        workers: u32,

        /// Reset state between tests
        #[arg(long)]
        reset: bool,
    },

    /// Deploy with blue-green strategy
    Deploy {
        /// Deployment strategy
        #[arg(long, default_value = "blue-green")]
        strategy: String,

        /// Path to new config
        #[arg(long)]
        config: Option<String>,
    },

    /// Rollback to previous deployment
    Rollback,
}
