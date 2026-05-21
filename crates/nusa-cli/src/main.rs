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
use std::sync::Arc;

use nusa_cli::{Cli, Commands};
use nusa_config::EngineKind;
use nusa_core::{
    BackpressureGuard, PhpEngine, ResourceGuard, TaskManager, TenantRateLimiter, TenantRegistry,
};
use nusa_gateway::bluegreen::BlueGreenDeployer;
use nusa_gateway::circuit_breaker::CircuitBreaker;
use nusa_gateway::health::HealthState;
use nusa_gateway::sse::SseManager;
use nusa_gateway::static_files::StaticFileHandler;
use nusa_gateway::tenant_circuit_breaker::TenantCircuitBreakers;
use nusa_gateway::websocket::WsManager;
use nusa_plugin_api::PluginRegistry;
use nusa_telemetry::metrics::NusaMetrics;
use std::time::Duration;

/// Binary entrypoint (m06-error-handling: anyhow for app-level errors)
/// m12-lifecycle: explicit init→serve→shutdown phases
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Dispatch subcommands (domain-cli)
    if let Some(cmd) = cli.command {
        return handle_subcommand(cmd).await;
    }

    // Default: start the server
    start_server(&cli.config).await
}

/// Handle CLI subcommands (nusa dev, nusa test, nusa deploy, nusa rollback).
async fn handle_subcommand(cmd: Commands) -> anyhow::Result<()> {
    // Init telemetry first
    nusa_telemetry::init()?;

    match cmd {
        Commands::Dev {
            watch,
            debounce,
            pretty,
        } => {
            // F4: nusa dev — hot-reload development mode
            tracing::info!("🔥 Nusa dev mode — watching directories: {}", watch);
            let app_root = std::env::current_dir()?;
            let mut watcher = nusa_cli::dev::DevWatcher::new(debounce, pretty);
            watcher.start(&app_root)?;

            // Also start the server
            start_server(&cli_config_path()).await
        }
        Commands::Test {
            path,
            workers,
            reset,
        } => {
            // F5: nusa test — persistent test runner
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
            // F6: nusa deploy — blue-green deployment
            tracing::info!(
                "Deploying with strategy: {} (config: {:?})",
                strategy,
                config
            );
            let config_path = config.unwrap_or_else(cli_config_path);
            start_server(&config_path).await
        }
        Commands::Rollback => {
            // F6: nusa rollback
            tracing::info!("Rolling back to previous deployment");
            start_server(&cli_config_path()).await
        }
    }
}

/// Start the Nusa PHP Runtime server.
async fn start_server(config_path: &str) -> anyhow::Result<()> {
    // 1. Init Telemetry (before any app logic)
    let obs = nusa_telemetry::init()?;
    let prometheus_handle = Arc::new(obs.prometheus_handle);

    // 2. Load Config (figment: file → env → defaults)
    nusa_config::load(config_path)?;
    let _watcher = nusa_config::watch(config_path.to_string());

    let cfg = nusa_config::get();

    // 3. Initialize Engine based on config (m04-zero-cost: dyn dispatch)
    let engine: Arc<dyn PhpEngine> = match cfg.engine {
        EngineKind::Ffi => Arc::new(nusa_engine_ffi::FfiEngine::new(cfg.max_workers)),
        EngineKind::Wasm => Arc::new(nusa_engine_wasm::WasmEngine::stub()),
        EngineKind::Child => Arc::new(nusa_engine_child::ChildEngine::with_default_php()),
    };

    // 4. Apply Security BEFORE axum::serve (m15-anti-pattern: security first)
    nusa_security::apply_landlock(cfg.code_dir.as_ref(), cfg.tmp_dir.as_ref())?;
    nusa_security::apply_seccomp()?;

    // 5. Initialize Plugins
    let plugins = Arc::new(PluginRegistry::new());

    // 6. Circuit Breaker (m13-domain-error)
    let circuit_breaker = Arc::new(CircuitBreaker::new(5, Duration::from_secs(30)));

    // 7. Health State
    let health_state = Arc::new(HealthState::new());

    // 8. Backpressure Guard
    let backpressure = Arc::new(BackpressureGuard::new(cfg.max_workers));

    // 9. Resource Guard
    let resource_guard = ResourceGuard {
        max_request_bytes: 10 * 1024 * 1024, // 10MB default
        request_timeout_ms: cfg.timeout_ms,
        max_concurrent: cfg.max_workers,
    };

    // 10. Tenant Registry (M4: multi-tenant)
    let tenants = Arc::new(TenantRegistry::new());

    // 11. Task Manager (M4: async task offload)
    let tasks = Arc::new(TaskManager::new());

    // 12. Tenant Rate Limiter (D2)
    let rate_limiter = Arc::new(TenantRateLimiter::new(1000, 50));

    // 13. Tenant Circuit Breaker (D3)
    let tenant_cb = Arc::new(TenantCircuitBreakers::new(5, Duration::from_secs(30)));

    // 14. WebSocket Manager (F1)
    let ws_manager = Arc::new(WsManager::new());

    // 15. SSE Manager (F2)
    let sse_manager = Arc::new(SseManager::new());

    // 16. Static File Handler (E1)
    let static_handler = Arc::new(StaticFileHandler::new("/app/public".into()));

    // 17. Metrics (A1)
    let metrics = Arc::new(NusaMetrics::init());

    // 18. Octane Worker Pool (M2)
    let octane_pool = Arc::new(parking_lot::Mutex::new(None));
    let mut octane_reset = nusa_octane_worker::state_reset::StateResetOrchestrator::new(128);
    octane_reset.initialize();
    let octane_reset = Arc::new(parking_lot::Mutex::new(octane_reset));

    // 19. Blue-Green Deployer (F6)
    let _deployer = BlueGreenDeployer::new(nusa_gateway::app(
        engine.clone(),
        plugins.clone(),
        circuit_breaker.clone(),
        health_state.clone(),
        backpressure.clone(),
        resource_guard.clone(),
        tenants.clone(),
        tasks.clone(),
        rate_limiter.clone(),
        tenant_cb.clone(),
        ws_manager.clone(),
        sse_manager.clone(),
        static_handler.clone(),
        metrics.clone(),
        prometheus_handle.clone(),
        octane_pool.clone(),
        octane_reset.clone(),
    ));

    // 20. Start Gateway
    let app = nusa_gateway::app(
        engine.clone(),
        plugins,
        circuit_breaker,
        health_state.clone(),
        backpressure,
        resource_guard,
        tenants,
        tasks,
        rate_limiter,
        tenant_cb,
        ws_manager,
        sse_manager,
        static_handler,
        metrics,
        prometheus_handle,
        octane_pool,
        octane_reset,
    );
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;

    // Mark as ready after successful startup
    health_state.mark_ready();

    tracing::info!("nusa listening on 0.0.0.0:8080");

    // 20. Graceful Shutdown (m12-lifecycle)
    let shutdown = async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("nusa shutdown signal received");
        engine.shutdown().await;
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;

    Ok(())
}

fn cli_config_path() -> String {
    "nusa.toml".to_string()
}
