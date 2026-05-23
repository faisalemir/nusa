#![deny(unsafe_code)]
#![warn(clippy::all)]
// missing_docs enabled as warning; allowing at crate level until full doc pass is complete
#![allow(missing_docs)]

//! Configuration management with hot-reload support.
//!
//! Skills applied:
//! - `m03-mutability`: ArcSwap for atomic config swap without Mutex
//! - `m12-lifecycle`: Load → watch → reload lifecycle
//! - `m07-concurrency`: LazyLock for global config initialization

use std::path::Path;
use std::sync::LazyLock;

use arc_swap::ArcSwap;
use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub mod laravel;

/// Runtime configuration for the Nusa PHP runtime.
///
/// Loaded from TOML file with environment variable overrides (`NUSA_` prefix).
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct RuntimeConfig {
    /// PHP engine type: FFI, WASM, or Child.
    pub engine: EngineKind,
    /// Maximum number of concurrent PHP workers.
    pub max_workers: usize,
    /// Per-request timeout in milliseconds.
    pub timeout_ms: u64,
    /// WASM sandbox memory limit in megabytes.
    pub wasm_memory_mb: u64,
    /// Root directory for the virtual filesystem.
    pub vfs_root: String,
    /// Directory containing the Laravel application code.
    pub code_dir: String,
    /// Temporary directory for PHP processes.
    pub tmp_dir: String,
    /// Enable hot-reload on config file changes.
    pub hot_reload: bool,
    /// Number of Octane PHP workers (0 = disabled, > 0 = Octane mode).
    #[serde(default)]
    pub octane_workers: usize,
    /// Octane worker max memory in MB before recycling.
    #[serde(default = "default_max_memory_mb")]
    pub octane_max_memory_mb: u64,
    /// Octane worker max requests before recycling.
    #[serde(default = "default_max_requests")]
    pub octane_max_requests: u64,
    /// HTTP listen address (e.g. `0.0.0.0:8080`).
    #[serde(default = "default_bind")]
    pub bind: String,
    /// Static file document root; when empty, uses `{code_dir}/public`.
    #[serde(default)]
    pub static_root: String,
    /// PHP binary for `engine = "child"` (e.g. `php` or `php84` on Alpine).
    #[serde(default = "default_php_binary")]
    pub php_binary: String,
    /// Child-engine IPC bootstrap script; empty uses `index.php` in the process working directory.
    #[serde(default)]
    pub php_bootstrap: String,
    /// TLS / ACME settings (experimental).
    #[serde(default)]
    pub tls: TlsSettings,
    /// Redis broadcast bridge (empty URL = disabled).
    #[serde(default)]
    pub redis: RedisSettings,
    /// HTTP/3 QUIC listener (experimental).
    #[serde(default)]
    pub quic: QuicSettings,
    /// Registered tenants (`[[tenants]]` in TOML); empty = open (no registry gate).
    #[serde(default)]
    pub tenants: Vec<TenantEntry>,
}

/// Multi-tenant entry in `nusa.toml` (`[[tenants]]`).
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct TenantEntry {
    /// Tenant identifier (`X-Tenant-Id` / subdomain).
    pub id: String,
    /// Web root for this tenant (Laravel `public/` or equivalent).
    pub vfs_root: String,
    /// When false, gateway returns 403 for this tenant.
    #[serde(default = "default_tenant_enabled")]
    pub enabled: bool,
    #[serde(default = "default_tenant_max_memory_mb")]
    pub max_memory_mb: u64,
    #[serde(default = "default_tenant_rpm")]
    pub max_requests_per_minute: u64,
}

fn default_tenant_enabled() -> bool {
    true
}

fn default_tenant_max_memory_mb() -> u64 {
    512
}

fn default_tenant_rpm() -> u64 {
    1000
}

/// TLS / ACME configuration (experimental; full ACME is post-GA).
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct TlsSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub acme_email: String,
}

/// Redis pub/sub broadcast bridge.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct RedisSettings {
    /// Redis URL; empty disables the bridge.
    #[serde(default)]
    pub broadcast_url: String,
}

/// HTTP/3 QUIC listener (experimental).
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct QuicSettings {
    #[serde(default)]
    pub enabled: bool,
    /// UDP listen address when QUIC is enabled.
    #[serde(default = "default_quic_bind")]
    pub bind: String,
}

fn default_quic_bind() -> String {
    "0.0.0.0:443".into()
}

fn default_max_memory_mb() -> u64 {
    512
}

fn default_max_requests() -> u64 {
    1000
}

fn default_bind() -> String {
    "0.0.0.0:8080".into()
}

fn default_php_binary() -> String {
    "php".into()
}

/// Resolved static file root (explicit `static_root` or Laravel `public/` under `code_dir`).
pub fn effective_static_root(cfg: &RuntimeConfig) -> String {
    if !cfg.static_root.is_empty() {
        return cfg.static_root.clone();
    }
    let base = cfg.code_dir.trim_end_matches('/');
    format!("{base}/public")
}

/// PHP engine execution kind.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub enum EngineKind {
    /// PHP embedded via FFI (libphp ZTS).
    Ffi,
    /// PHP running inside WASM sandbox (wasmtime).
    Wasm,
    /// PHP running as child processes.
    Child,
}

/// Built-in defaults (Laravel-friendly container layout).
pub fn default_config() -> RuntimeConfig {
    laravel::container_defaults()
}

pub(crate) fn builtin_defaults() -> RuntimeConfig {
    RuntimeConfig {
        engine: EngineKind::Child,
        max_workers: 4,
        timeout_ms: 30_000,
        wasm_memory_mb: 256,
        vfs_root: String::new(),
        code_dir: "/app".into(),
        tmp_dir: "/tmp/nusa".into(),
        hot_reload: true,
        octane_workers: 0, // disabled by default
        octane_max_memory_mb: 512,
        octane_max_requests: 1000,
        bind: default_bind(),
        static_root: String::new(),
        php_binary: default_php_binary(),
        php_bootstrap: String::new(),
        tls: TlsSettings::default(),
        redis: RedisSettings::default(),
        quic: QuicSettings::default(),
        tenants: Vec::new(),
    }
}

static CONFIG: LazyLock<ArcSwap<RuntimeConfig>> =
    LazyLock::new(|| ArcSwap::from_pointee(default_config()));

/// How configuration was resolved at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigResolution {
    /// Loaded from this TOML path (may not exist if misconfigured).
    File(std::path::PathBuf),
    /// No file; defaults + `NUSA_*` environment only (12-factor / containers).
    EnvOnly,
}

/// Resolve config file: CLI path → `NUSA_CONFIG` → `./nusa.toml` → `/etc/nusa/nusa.toml`.
pub fn resolve_config_path(cli_arg: &str) -> anyhow::Result<ConfigResolution> {
    let cli = Path::new(cli_arg);
    if cli.exists() {
        return Ok(ConfigResolution::File(cli.to_path_buf()));
    }

    if let Ok(path) = std::env::var("NUSA_CONFIG") {
        let p = Path::new(&path);
        if p.exists() {
            return Ok(ConfigResolution::File(p.to_path_buf()));
        }
        anyhow::bail!("NUSA_CONFIG points to missing file: {path}");
    }

    for candidate in ["nusa.toml", "/etc/nusa/nusa.toml"] {
        let p = Path::new(candidate);
        if p.exists() {
            return Ok(ConfigResolution::File(p.to_path_buf()));
        }
    }

    if cli_arg != "nusa.toml" {
        anyhow::bail!("config file not found: {cli_arg}");
    }

    Ok(ConfigResolution::EnvOnly)
}

fn build_figment(config_path: Option<&str>) -> Figment {
    let mut figment = Figment::new().merge(Serialized::defaults(builtin_defaults()));

    if let Some(path) = config_path
        && Path::new(path).exists()
    {
        figment = figment.merge(Toml::file(path));
    }

    // `NUSA_MAX_WORKERS` → `max_workers`; `NUSA_TLS__ENABLED` → `tls.enabled`.
    figment.merge(Env::prefixed("NUSA_").split("__"))
}

/// Reject corrupt TOML before merge so hot-reload keeps the last good snapshot.
fn ensure_toml_file_valid(path: &str) -> anyhow::Result<()> {
    let content = std::fs::read_to_string(path)?;
    toml::from_str::<toml::Table>(&content)
        .map_err(|e| anyhow::anyhow!("invalid TOML in {path}: {e}"))?;
    Ok(())
}

fn validate_config(cfg: &RuntimeConfig) -> anyhow::Result<()> {
    if cfg.max_workers == 0 {
        anyhow::bail!("max_workers must be > 0");
    }
    if cfg.bind.parse::<std::net::SocketAddr>().is_err() {
        anyhow::bail!("invalid bind address: {}", cfg.bind);
    }
    if cfg.quic.enabled && cfg.quic.bind.parse::<std::net::SocketAddr>().is_err() {
        anyhow::bail!("invalid quic bind address: {}", cfg.quic.bind);
    }
    Ok(())
}

/// Load from optional TOML path + `NUSA_*` env overrides (`NUSA_*` wins over file).
pub fn load_sources(config_path: Option<&str>) -> anyhow::Result<()> {
    if let Some(path) = config_path
        && Path::new(path).exists()
    {
        ensure_toml_file_valid(path)?;
    }
    let mut cfg: RuntimeConfig = build_figment(config_path).extract()?;
    laravel::normalize_paths(&mut cfg);
    validate_config(&cfg)?;
    CONFIG.store(Arc::new(cfg));
    Ok(())
}

/// Load configuration from a TOML file path.
///
/// Merges defaults → file → `NUSA_`-prefixed environment variables (env wins).
/// Use [`load_sources`] when the file may be absent (container env-only).
pub fn load(path: &str) -> anyhow::Result<()> {
    if !std::path::Path::new(path).exists() {
        return Err(anyhow::anyhow!("config file not found: {path}"));
    }
    load_sources(Some(path))
}

/// Get the current active configuration.
///
/// Returns an `Arc` clone of the currently stored config via `ArcSwap`.
pub fn get() -> Arc<RuntimeConfig> {
    CONFIG.load().clone()
}

/// Start watching config file for changes.
/// Returns a JoinHandle so the caller can cancel on shutdown.
pub fn watch(path: String) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if !get().hot_reload || path.is_empty() || !std::path::Path::new(&path).exists() {
            return;
        }

        use notify::{EventKind, Watcher};

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let mut watcher =
            match notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res
                    && matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_))
                {
                    tracing::info!("config file changed, reloading: {:?}", event.paths);
                    let _ = tx.send(());
                }
            }) {
                Ok(w) => w,
                Err(e) => {
                    tracing::error!(err = %e, "failed to create config watcher");
                    return;
                }
            };

        if let Err(e) = watcher.watch(
            std::path::Path::new(&path),
            notify::RecursiveMode::NonRecursive,
        ) {
            tracing::error!(err = %e, "failed to watch config file");
            return;
        }

        // Keep watcher alive while listening for reload signals
        while let Some(()) = rx.recv().await {
            if let Err(e) = load_sources(Some(path.as_str())) {
                tracing::error!(err = %e, "failed to reload config");
            } else {
                tracing::info!("config reloaded successfully");
            }
        }
    })
}
