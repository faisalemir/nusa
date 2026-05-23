//! Server startup, blue-green deploy, and optional feature wiring.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use nusa_config::{EngineKind, RuntimeConfig};
use nusa_core::{
    BackpressureGuard, PhpEngine, ResourceGuard, TaskManager, TenantRateLimiter, TenantRegistry,
};
use nusa_gateway::acme::{AcmeConfig, AcmeProvider, TlsService};
use nusa_gateway::bluegreen::{BlueGreenDeployer, serving_router};
use nusa_gateway::broadcast::BroadcastBridge;
use nusa_gateway::circuit_breaker::CircuitBreaker;
use nusa_gateway::health::HealthState;
use nusa_gateway::quic;
use nusa_gateway::sse::SseManager;
use nusa_gateway::static_files::StaticFileHandler;
use nusa_gateway::tenant_circuit_breaker::TenantCircuitBreakers;
use nusa_gateway::websocket::WsManager;
use nusa_octane_worker::pool::WorkerPool;
use nusa_octane_worker::state_reset::StateResetOrchestrator;
use nusa_plugin_api::PluginRegistry;
use nusa_telemetry::metrics::NusaMetrics;
use parking_lot::Mutex;
use tokio::sync::Mutex as AsyncMutex;

use crate::deploy;
use crate::dev::{DevAction, DevWatcher};
use crate::octane_pool;

/// How to start the HTTP server.
#[derive(Debug, Clone)]
pub enum StartMode {
    /// Normal or `nusa dev`.
    Default,
    /// `nusa deploy`: blue config active, green config on standby then switch.
    Deploy {
        blue_config: String,
        green_config: String,
    },
    /// `nusa rollback`: start using previous config from `.nusa/deploy-state.toml`.
    Rollback,
}

/// Shared handles for dev hot-reload.
#[derive(Clone)]
pub struct ServerHandles {
    pub octane_pool: Arc<AsyncMutex<Option<WorkerPool>>>,
    pub config_path: String,
}

/// Build `PhpEngine` from the current global config.
pub fn build_engine(cfg: &RuntimeConfig) -> anyhow::Result<Arc<dyn PhpEngine>> {
    let engine: Arc<dyn PhpEngine> = match cfg.engine {
        EngineKind::Ffi => Arc::new(nusa_engine_ffi::FfiEngine::new(cfg.max_workers)),
        EngineKind::Wasm => {
            if std::env::var("NUSA_ALLOW_WASM_STUB").is_err() {
                anyhow::bail!(
                    "engine=wasm is dev-only (WasmEngine::stub); use engine=child for production \
                     or set NUSA_ALLOW_WASM_STUB=1 for local experiments"
                );
            }
            Arc::new(nusa_engine_wasm::WasmEngine::stub())
        }
        EngineKind::Child => {
            let php_binary = std::path::PathBuf::from(if cfg.php_binary.is_empty() {
                "php"
            } else {
                cfg.php_binary.as_str()
            });
            let bootstrap = if cfg.php_bootstrap.is_empty() {
                std::path::PathBuf::from("index.php")
            } else {
                std::path::PathBuf::from(cfg.php_bootstrap.as_str())
            };
            Arc::new(nusa_engine_child::ChildEngine::new(php_binary, bootstrap))
        }
    };
    Ok(engine)
}

/// Initialize Octane pool from config (may be `None`).
pub async fn init_pool_from_config(cfg: &RuntimeConfig) -> anyhow::Result<Option<WorkerPool>> {
    octane_pool::init_octane_pool(
        cfg.octane_workers,
        std::path::PathBuf::from(&cfg.code_dir),
        cfg.octane_max_memory_mb,
        cfg.octane_max_requests,
    )
    .await
}

/// Recycle Octane workers after config or code changes (dev / hot-reload).
pub async fn recycle_octane_pool(pool: &Arc<AsyncMutex<Option<WorkerPool>>>) -> anyhow::Result<()> {
    let cfg = nusa_config::get();
    if cfg.octane_workers == 0 {
        return Ok(());
    }
    let mut guard = pool.lock().await;
    if let Some(mut existing) = guard.take() {
        existing.shutdown().await?;
    }
    *guard = init_pool_from_config(&cfg).await?;
    if guard.is_some() {
        tracing::info!(
            "Octane worker pool recycled ({} workers)",
            cfg.octane_workers
        );
    }
    Ok(())
}

struct AppComponents {
    engine: Arc<dyn PhpEngine>,
    plugins: Arc<PluginRegistry>,
    circuit_breaker: Arc<CircuitBreaker>,
    health_state: Arc<HealthState>,
    backpressure: Arc<BackpressureGuard>,
    resource_guard: Arc<ResourceGuard>,
    tenants: Arc<TenantRegistry>,
    tasks: Arc<TaskManager>,
    rate_limiter: Arc<TenantRateLimiter>,
    tenant_cb: Arc<TenantCircuitBreakers>,
    ws_manager: Arc<WsManager>,
    sse_manager: Arc<SseManager>,
    static_handler: Arc<StaticFileHandler>,
    metrics: Arc<NusaMetrics>,
    prometheus_handle: Arc<metrics_exporter_prometheus::PrometheusHandle>,
    octane_pool: Arc<AsyncMutex<Option<WorkerPool>>>,
    octane_reset: Arc<Mutex<StateResetOrchestrator>>,
}

async fn build_components(
    prometheus_handle: Arc<metrics_exporter_prometheus::PrometheusHandle>,
) -> anyhow::Result<AppComponents> {
    let cfg = nusa_config::get();

    let engine = build_engine(&cfg)?;
    nusa_security::apply_landlock(cfg.code_dir.as_ref(), cfg.tmp_dir.as_ref())?;
    if std::env::var("NUSA_SKIP_SECCOMP").is_ok() {
        tracing::warn!(
            "NUSA_SKIP_SECCOMP is set — seccomp filter not installed (bench/dev only)"
        );
        nusa_security::verify_seccomp_filter()?;
    } else {
        nusa_security::apply_seccomp()?;
    }

    let plugins = Arc::new(PluginRegistry::new());
    let circuit_breaker = Arc::new(CircuitBreaker::new(5, Duration::from_secs(30)));
    let health_state = Arc::new(HealthState::new());
    let backpressure = Arc::new(BackpressureGuard::new(cfg.max_workers));
    let resource_guard = Arc::new(ResourceGuard {
        max_request_bytes: 10 * 1024 * 1024,
        request_timeout_ms: cfg.timeout_ms,
        max_concurrent: cfg.max_workers,
    });
    let tenants = Arc::new(TenantRegistry::new());
    let tasks = Arc::new(TaskManager::new());
    let rate_limiter = Arc::new(TenantRateLimiter::new(1000, 50));
    let tenant_cb = Arc::new(TenantCircuitBreakers::new(5, Duration::from_secs(30)));
    let ws_manager = Arc::new(WsManager::new());
    let sse_manager = Arc::new(SseManager::new());
    let static_root = nusa_config::effective_static_root(&cfg);
    let static_handler = Arc::new(StaticFileHandler::new(static_root.into()));
    let metrics = Arc::new(NusaMetrics::init());

    let octane_pool = match init_pool_from_config(&cfg).await? {
        Some(pool) => {
            tracing::info!(
                "Octane worker pool ready with {} workers",
                cfg.octane_workers
            );
            Some(pool)
        }
        None => None,
    };
    let octane_pool = Arc::new(AsyncMutex::new(octane_pool));
    let mut octane_reset = StateResetOrchestrator::new(128);
    octane_reset.initialize();
    let octane_reset = Arc::new(Mutex::new(octane_reset));

    Ok(AppComponents {
        engine,
        plugins,
        circuit_breaker,
        health_state,
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
    })
}

fn build_router(components: &AppComponents) -> Router {
    nusa_gateway::app(
        components.engine.clone(),
        components.plugins.clone(),
        components.circuit_breaker.clone(),
        components.health_state.clone(),
        components.backpressure.clone(),
        (*components.resource_guard).clone(),
        components.tenants.clone(),
        components.tasks.clone(),
        components.rate_limiter.clone(),
        components.tenant_cb.clone(),
        components.ws_manager.clone(),
        components.sse_manager.clone(),
        components.static_handler.clone(),
        components.metrics.clone(),
        components.prometheus_handle.clone(),
        components.octane_pool.clone(),
        components.octane_reset.clone(),
    )
}

fn spawn_optional_services(components: &AppComponents) {
    let cfg = nusa_config::get();

    if cfg.tls.enabled {
        let tls = TlsService::new(AcmeConfig {
            enabled: true,
            provider: AcmeProvider::LetsEncrypt,
            email: cfg.tls.acme_email.clone(),
            cache_dir: std::path::PathBuf::from("/var/lib/nusa/certs"),
            auto_redirect_http_to_https: true,
        });
        tokio::spawn(async move {
            tracing::warn!(
                "TLS/ACME enabled (experimental): certificate automation is not fully wired; \
                 use a reverse proxy or manual certs for production"
            );
            let _ = tls;
            tokio::time::sleep(Duration::from_secs(u64::MAX)).await;
        });
    }

    let redis_url = cfg.redis.broadcast_url.trim();
    if !redis_url.is_empty() {
        let ws = components.ws_manager.clone();
        let sse = components.sse_manager.clone();
        let url = redis_url.to_string();
        tokio::spawn(async move {
            match BroadcastBridge::new(&url).await {
                Ok(bridge) => {
                    if let Err(e) = bridge.run_listener(&ws, &sse).await {
                        tracing::error!(err = %e, "Redis broadcast listener exited");
                    }
                }
                Err(e) => {
                    tracing::error!(err = %e, "Redis broadcast bridge failed to connect");
                }
            }
        });
    }

    if cfg.quic.enabled {
        tracing::warn!(
            "QUIC enabled (experimental): HTTP/3 stream handling is not production-ready; \
             UDP listener accepts connections only on {}",
            cfg.quic.bind
        );
        match quic::spawn_experimental(&cfg.quic.bind, axum::Router::new()) {
            Ok(_handle) => {
                tracing::info!("QUIC experimental listener spawned on {}", cfg.quic.bind)
            }
            Err(e) => tracing::error!(err = %e, "failed to spawn QUIC listener"),
        }
    }
}

fn spawn_dev_actions(watcher: DevWatcher, handles: ServerHandles) -> tokio::task::JoinHandle<()> {
    let mut rx = watcher.subscribe();
    tokio::spawn(async move {
        while let Ok(action) = rx.recv().await {
            match action {
                DevAction::ReloadConfig => {
                    if let Err(e) = nusa_config::load(&handles.config_path) {
                        tracing::error!(err = %e, "dev: config reload failed");
                    } else {
                        tracing::info!("dev: config reloaded");
                    }
                }
                DevAction::RecycleWorkers | DevAction::InvalidateOpCache => {
                    if let Err(e) = recycle_octane_pool(&handles.octane_pool).await {
                        tracing::error!(err = %e, "dev: worker recycle failed");
                    }
                }
                DevAction::ClearViewCache => {
                    tracing::info!(
                        "dev: view cache clear requested (no-op in v0.1.0; restart or touch storage/framework/views)"
                    );
                }
            }
        }
    })
}

/// Start the gateway HTTP server.
pub async fn start_server(
    config_path: &str,
    mode: StartMode,
    dev_watcher: Option<DevWatcher>,
) -> anyhow::Result<()> {
    let obs = nusa_telemetry::init()?;
    let prometheus_handle = Arc::new(obs.prometheus_handle);

    let (blue_path, green_path, rollback_path) = match &mode {
        StartMode::Default => (config_path.to_string(), None, None),
        StartMode::Deploy {
            blue_config,
            green_config,
        } => (blue_config.clone(), Some(green_config.clone()), None),
        StartMode::Rollback => {
            let state = deploy::read_deploy_state()?;
            (
                state.previous_config.clone(),
                None,
                Some(state.active_config),
            )
        }
    };

    nusa_config::load(&blue_path)?;
    let _watcher = nusa_config::watch(blue_path.clone());

    let components = build_components(prometheus_handle.clone()).await?;
    spawn_optional_services(&components);

    let handles = ServerHandles {
        octane_pool: components.octane_pool.clone(),
        config_path: blue_path.clone(),
    };

    if let Some(watcher) = dev_watcher {
        let _dev_task = spawn_dev_actions(watcher, handles);
    }

    let initial_router = build_router(&components);
    let deployer = Arc::new(BlueGreenDeployer::new(initial_router));

    let mut engine_shutdown = components.engine.clone();
    let mut octane_pool_shutdown = components.octane_pool.clone();
    let health_state = components.health_state.clone();

    if let Some(green_path) = green_path {
        nusa_config::load(&green_path)?;
        let green_components = build_components(prometheus_handle).await?;
        let green_router = build_router(&green_components);
        deployer.prepare_deployment("green", green_router);
        deployer.mark_standby_healthy();
        if deployer.health_check_standby() {
            deployer.switch();
            deploy::write_deploy_state(&blue_path, &green_path)?;
            tracing::info!("deploy: switched active slot to green ({green_path})");
            engine_shutdown = green_components.engine.clone();
            octane_pool_shutdown = green_components.octane_pool.clone();
        } else {
            anyhow::bail!("deploy: standby slot failed health check");
        }
    }

    if let Some(active_after_rollback) = rollback_path {
        tracing::info!(
            "rollback: starting with previous config {} (was active: {})",
            blue_path,
            active_after_rollback
        );
    }

    let app = serving_router(deployer);
    let cfg = nusa_config::get();
    let listener = tokio::net::TcpListener::bind(cfg.bind.as_str()).await?;
    health_state.mark_ready();
    tracing::info!("nusa listening on {}", cfg.bind);
    let shutdown = async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("nusa shutdown signal received");
        let pool_opt = octane_pool_shutdown.lock().await.take();
        if let Some(mut pool) = pool_opt {
            let _ = pool.shutdown().await;
        }
        engine_shutdown.shutdown().await;
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;

    Ok(())
}
