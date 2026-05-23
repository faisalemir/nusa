//! Nusa PHP Runtime CLI binary.
//!
//! Skills applied:
//! - `m06-error-handling`: anyhow for binary-level errors
//! - `m12-lifecycle`: explicit init→serve→shutdown phases
//! - `m15-anti-pattern`: Security applied before server start
//! - `domain-cli`: clap derive for argument parsing, subcommands

#![deny(unsafe_code)]
#![warn(clippy::all)]

use clap::Parser;

use nusa_cli::server::{StartMode, start_server};
use nusa_cli::{Cli, Commands};

/// Binary entrypoint (m06-error-handling: anyhow for app-level errors)
/// m12-lifecycle: explicit init→serve→shutdown phases
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if let Some(cmd) = cli.command {
        return handle_subcommand(cmd).await;
    }

    start_server(&cli.config, StartMode::Default, None).await
}

/// Handle CLI subcommands (nusa dev, nusa test, nusa deploy, nusa rollback).
async fn handle_subcommand(cmd: Commands) -> anyhow::Result<()> {
    nusa_telemetry::init()?;

    match cmd {
        Commands::Dev {
            watch: _watch,
            debounce,
            pretty,
        } => {
            tracing::info!("Nusa dev mode — hot-reload enabled");
            let app_root = std::env::current_dir()?;
            let mut watcher = nusa_cli::dev::DevWatcher::new(debounce, pretty);
            watcher.start(&app_root)?;
            start_server(&cli_config_path(), StartMode::Default, Some(watcher)).await
        }
        Commands::Test {
            path,
            workers,
            reset,
        } => {
            tracing::info!("Running tests from {} with {} workers", path, workers);
            let mut runner = nusa_cli::test::TestRunner::new(nusa_cli::test::TestConfig {
                workers,
                test_path: path.into(),
                reset_between_tests: reset,
            });
            let result = runner.run_tests().await?;
            tracing::info!("{}", result);
            runner.shutdown().await?;
            Ok(())
        }
        Commands::Deploy { strategy, config } => {
            tracing::info!("Deploying with strategy: {}", strategy);
            let blue = cli_config_path();
            let green = config.unwrap_or_else(cli_config_path);
            start_server(
                &blue,
                StartMode::Deploy {
                    blue_config: blue.clone(),
                    green_config: green,
                },
                None,
            )
            .await
        }
        Commands::Rollback => {
            tracing::info!("Rolling back to previous deployment config");
            start_server("", StartMode::Rollback, None).await
        }
        Commands::Init {
            octane,
            output,
            force,
        } => run_init(octane, &output, force),
    }
}

fn run_init(octane: bool, output: &str, force: bool) -> anyhow::Result<()> {
    let out = std::path::Path::new(output);
    if out.exists() && !force {
        anyhow::bail!(
            "{output} already exists; use --force to overwrite or run `nusa` with NUSA_* env only"
        );
    }

    let root = nusa_config::laravel::detect_project_root(&std::env::current_dir()?)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no Laravel project found (artisan missing); run from your app root or set paths via NUSA_CODE_DIR"
            )
        })?;

    let workers = if octane { 4 } else { 0 };
    let contents = nusa_config::laravel::render_starter_toml(&root, workers);
    std::fs::write(out, contents)?;
    println!(
        "Wrote {} (code_dir={})",
        out.display(),
        root.display()
    );
    println!("Start: nusa --config {}", out.display());
    println!("Or container-only: set NUSA_CODE_DIR and NUSA_OCTANE_WORKERS (see docs/public/laravel/docker.md)");
    Ok(())
}

fn cli_config_path() -> String {
    "nusa.toml".to_string()
}
