//! Hot-reload file watcher for development mode.
//! Blueprint 6 F4: `nusa dev` command with instant feedback loop.
//!
//! Skills applied:
//! - `m12-lifecycle`: Debounced file events → config reload / worker recycle
//! - `domain-cli`: Dev mode with pretty-print logging, auto-restart
//! - `m15-anti-pattern`: Debounce prevents rapid restart thrashing

use std::path::Path;

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tokio::sync::broadcast;
use tokio::time::Duration;
use tracing::info;

/// Action triggered by file change events.
#[derive(Debug, Clone)]
pub enum DevAction {
    /// Reload configuration from file (e.g. .env change).
    ReloadConfig,
    /// Recycle worker pool (e.g. config/*.php change).
    RecycleWorkers,
    /// Invalidate OPcache and recycle workers (e.g. app code change).
    InvalidateOpCache,
    /// Clear view cache (e.g. blade template change).
    ClearViewCache,
}

/// Watched directory patterns for development.
const WATCH_PATTERNS: &[&str] = &["app", "config", "routes", "resources/views", ".env"];

/// Ignored directories (never watch these).
const IGNORE_DIRS: &[&str] = &["vendor", "node_modules", ".git", "storage", "bootstrap/cache"];

/// File watcher for hot-reload in development mode.
///
/// m12-lifecycle: File events trigger specific lifecycle actions without full restart.
/// m15-anti-pattern: Debounce window prevents rapid restarts from bulk file operations.
pub struct DevWatcher {
    watcher: Option<RecommendedWatcher>,
    pub debounce_ms: u64,
    pub pretty: bool,
    action_tx: broadcast::Sender<DevAction>,
}

impl DevWatcher {
    pub fn new(debounce_ms: u64, pretty: bool) -> Self {
        let (action_tx, _) = broadcast::channel(32);
        Self {
            watcher: None,
            debounce_ms,
            pretty,
            action_tx,
        }
    }

    /// Subscribe to dev watcher actions.
    /// Returns a receiver that will get signals when file changes trigger actions.
    pub fn subscribe(&self) -> broadcast::Receiver<DevAction> {
        self.action_tx.subscribe()
    }

    /// Start watching configured directories.
    /// m15-anti-pattern: Only watches app-level directories, ignores vendor/node_modules.
    pub fn start(&mut self, app_root: &Path) -> anyhow::Result<()> {
        let debounce_ms = self.debounce_ms;
        let pretty = self.pretty;
        let app_root_clone = app_root.to_path_buf();
        let action_tx = self.action_tx.clone();

        let (tx, mut rx) = mpsc::unbounded_channel::<Event>();

        // Spawn debounce task
        tokio::spawn(async move {
            Self::debounce_loop(&mut rx, debounce_ms, &app_root_clone, pretty, action_tx).await;
        });

        let watcher = notify::recommended_watcher(move |res: Result<Event, _>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        })?;

        let mut watcher = watcher;

        for pattern in WATCH_PATTERNS {
            let dir = app_root.join(pattern);
            if dir.exists() && dir.is_dir() {
                watcher.watch(&dir, RecursiveMode::Recursive)?;
                info!("Watching: {:?}", dir);
            }
        }

        self.watcher = Some(watcher);

        if self.pretty {
            info!("Nusa dev mode started — watching for changes");
        } else {
            info!("Nusa dev mode started — watching for changes");
        }

        Ok(())
    }

    /// Debounce loop: batch events within debounce_ms window, then handle.
    pub async fn debounce_loop(
        rx: &mut mpsc::UnboundedReceiver<Event>,
        debounce_ms: u64,
        app_root: &Path,
        pretty: bool,
        action_tx: broadcast::Sender<DevAction>,
    ) {
        use tokio::time::Instant;

        let mut pending_events = Vec::new();
        let mut deadline: Option<Instant> = None;

        loop {
            let debounce_dur = Duration::from_millis(debounce_ms);
            let sleep = if let Some(dead) = deadline {
                tokio::time::sleep_until(dead)
            } else {
                tokio::time::sleep(debounce_dur)
            };

            tokio::select! {
                Some(event) = rx.recv() => {
                    pending_events.push(event);
                    // Reset deadline on each new event
                    deadline = Some(Instant::now() + debounce_dur);
                }
                _ = async { sleep.await } => {
                    if pending_events.is_empty() {
                        continue;
                    }

                    let events: Vec<Event> = std::mem::take(&mut pending_events);
                    deadline = None;

                    for event in events {
                        Self::handle_event(&event, app_root, pretty, &action_tx);
                    }
                }
            }
        }
    }

    /// Handle a file change event with appropriate action (m12-lifecycle).
    pub fn handle_event(event: &Event, _app_root: &Path, _pretty: bool, action_tx: &broadcast::Sender<DevAction>) {
        for path in &event.paths {
            let path_str = path.to_string_lossy();

            // m15-anti-pattern: Skip ignored directories
            if IGNORE_DIRS.iter().any(|d| path_str.contains(d)) {
                continue;
            }

            if path_str.ends_with(".env") {
                // .env change → reload config via ArcSwap (no restart needed)
                info!(".env changed — reloading config (hot)");
                let _ = action_tx.send(DevAction::ReloadConfig);
            } else if path_str.contains("config/") && path_str.ends_with(".php") {
                // config/*.php change → graceful worker recycle
                info!("Config changed — recycling workers");
                let _ = action_tx.send(DevAction::RecycleWorkers);
            } else if path_str.contains("app/") && path_str.ends_with(".php") {
                // app/**/*.php change → OPcache invalidation + worker recycle
                info!("App code changed — invalidating OPcache + recycling");
                let _ = action_tx.send(DevAction::InvalidateOpCache);
            } else if path_str.contains("resources/views/") && path_str.ends_with(".blade.php") {
                // view change → clear view cache
                info!("View changed — clearing view cache");
                let _ = action_tx.send(DevAction::ClearViewCache);
            }
        }
    }
}
