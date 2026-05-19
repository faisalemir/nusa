#![deny(unsafe_code)]
#![warn(clippy::all)]

use clap::Parser;
use std::sync::Arc;

use phprt_cli::Cli;
use phprt_config::EngineKind;
use phprt_plugin_api::PluginRegistry;
use phprt_core::PhpEngine;
use phprt_gateway::circuit_breaker::CircuitBreaker;
use phprt_gateway::health::HealthState;
use phprt_core::{BackpressureGuard, ResourceGuard};
use std::time::Duration;

/// Binary entrypoint (m06-error-handling: anyhow for app-level errors)
/// m12-lifecycle: explicit init→serve→shutdown phases
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // 1. Init Telemetry (before any app logic)
    phprt_telemetry::init()?;

    // 2. Load Config (figment: file → env → defaults)
    phprt_config::load(&cli.config)?;
    let _watcher = phprt_config::watch(cli.config.clone());

    let cfg = phprt_config::get();

    // 3. Initialize Engine based on config (m04-zero-cost: dyn dispatch)
    let engine: Arc<dyn PhpEngine> = match cfg.engine {
        EngineKind::Ffi => Arc::new(phprt_engine_ffi::FfiEngine::new(cfg.max_workers)),
        EngineKind::Wasm => {
            // TODO: Read php.wasm file
            Arc::new(phprt_engine_wasm::WasmEngine::stub())
        }
        EngineKind::Child => {
            Arc::new(phprt_engine_child::ChildEngine::with_default_php())
        }
    };

    // 4. Apply Security BEFORE axum::serve (m15-anti-pattern: security first)
    phprt_security::apply_landlock(
        cfg.code_dir.as_ref(),
        cfg.tmp_dir.as_ref(),
    )?;
    phprt_security::apply_seccomp()?;

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
        max_request_bytes: cfg.max_workers * 1024 * 1024, // Simplified
        request_timeout_ms: cfg.timeout_ms,
        max_concurrent: cfg.max_workers,
    };

    // 10. Start Gateway
    let app = phprt_gateway::app(
        engine.clone(),
        plugins.clone(),
        circuit_breaker,
        health_state.clone(),
        backpressure,
        resource_guard,
    );
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;

    // Mark as ready after successful startup
    health_state.mark_ready();

    tracing::info!("listening on 0.0.0.0:8080");

    // 11. Graceful Shutdown (m12-lifecycle)
    let shutdown = async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("shutdown signal received");
        engine.shutdown().await;
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;

    Ok(())
}
