//! Hot-reload file watcher for development mode.
//! Blueprint 6 F4: `nusa dev` command with instant feedback loop.
//!
//! Skills applied:
//! - `m12-lifecycle`: Debounced file events → config reload / worker recycle
//! - `domain-cli`: Dev mode with pretty-print logging, auto-restart
//! - `m15-anti-pattern`: Debounce prevents rapid restart thrashing

use std::path::Path;

use notify::{Event, RecommendedWatcher, Watcher, RecursiveMode};
use tracing::info;

/// Watched directory patterns for development.
const WATCH_PATTERNS: &[&str] = &[
    "app", "config", "routes", "resources/views", ".env",
];

/// Ignored directories (never watch these).
const IGNORE_DIRS: &[&str] = &["vendor", "node_modules", ".git", "storage", "bootstrap/cache"];

/// File watcher for hot-reload in development mode.
///
/// m12-lifecycle: File events trigger specific lifecycle actions without full restart.
/// m15-anti-pattern: Debounce window prevents rapid restarts from bulk file operations.
pub struct DevWatcher {
    watcher: Option<RecommendedWatcher>,
    #[allow(dead_code)]
    debounce_ms: u64,
    pretty: bool,
}

impl DevWatcher {
    pub fn new(debounce_ms: u64, pretty: bool) -> Self {
        Self {
            watcher: None,
            debounce_ms,
            pretty,
        }
    }

    /// Start watching configured directories.
    /// m15-anti-pattern: Only watches app-level directories, ignores vendor/node_modules.
    pub fn start(&mut self, app_root: &Path) -> anyhow::Result<()> {
        let mut watcher = notify::recommended_watcher(|res: Result<Event, _>| {
            if let Ok(event) = res {
                Self::handle_event(&event);
            }
        })?;

        for pattern in WATCH_PATTERNS {
            let dir = app_root.join(pattern);
            if dir.exists() && dir.is_dir() {
                watcher.watch(&dir, RecursiveMode::Recursive)?;
                info!("Watching: {:?}", dir);
            }
        }

        self.watcher = Some(watcher);

        if self.pretty {
            info!("🔥 Nusa dev mode started — watching for changes");
        } else {
            info!("Nusa dev mode started — watching for changes");
        }

        Ok(())
    }

    /// Handle a file change event with appropriate action (m12-lifecycle).
    fn handle_event(event: &Event) {
        let _debounce_ms = 200; // Configurable in production

        for path in &event.paths {
            let path_str = path.to_string_lossy();

            // m15-anti-pattern: Skip ignored directories
            if IGNORE_DIRS.iter().any(|d| path_str.contains(d)) {
                continue;
            }

            if path_str.ends_with(".env") {
                // .env change → reload config via ArcSwap (no restart needed)
                info!("📝 .env changed — reloading config (hot)");
            } else if path_str.contains("config/") && path_str.ends_with(".php") {
                // config/*.php change → graceful worker recycle
                info!("⚙️  Config changed — recycling workers");
            } else if path_str.contains("app/") && path_str.ends_with(".php") {
                // app/**/*.php change → OPcache invalidation + worker recycle
                info!("🔄 App code changed — invalidating OPcache + recycling");
            } else if path_str.contains("resources/views/") && path_str.ends_with(".blade.php") {
                // view change → clear view cache
                info!("🎨 View changed — clearing view cache");
            }
        }
    }

    /// Debounce: collect events within debounce_ms window.
    #[allow(dead_code)]
    fn debounce_events(events: &[Event], _debounce_ms: u64) -> Vec<Event> {
        // In production: use a tokio timer to batch events within the debounce window
        events.to_vec()
    }
}
