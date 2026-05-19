#![deny(unsafe_code)]
#![warn(clippy::all)]

use clap::Parser;

#[derive(Parser)]
#[command(name = "phprt", about = "Nusa PHP Runtime")]
pub struct Cli {
    #[arg(short, long, default_value = "config.toml")]
    pub config: String,
}
