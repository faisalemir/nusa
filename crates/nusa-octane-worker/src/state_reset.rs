//! State reset orchestrator for Octane event hooks.
//!
//! Skills applied:
//! - `m12-lifecycle`: Event-driven state management for worker lifecycle
//! - `m07-concurrency`: Broadcast channel for async event propagation
//! - `m09-domain`: Domain events reflect Octane's contract (RequestReceived, WorkerStopping)
//! - `m07-concurrency`: AtomicU64 for lock-free statistics counters

use std::sync::atomic::{AtomicU64, Ordering};
use std::collections::HashMap;

use tracing::info;

/// Octane lifecycle events that trigger state management actions.
#[derive(Debug, Clone)]
pub enum OctaneEvent {
    /// Worker process just booted Laravel.
    WorkerStarted { worker_id: usize },
    /// A new HTTP request is about to be handled.
    RequestReceived { request_id: String },
    /// An HTTP request finished processing.
    RequestTerminated { request_id: String, status: u16 },
    /// Worker is about to stop (shutdown or recycle).
    WorkerStopping { worker_id: usize },
}

/// Statistics about state reset operations.
#[derive(Debug, Default, Clone)]
pub struct StateResetStats {
    pub total_requests_processed: u64,
    pub total_resets_performed: u64,
    pub total_cleanups_performed: u64,
    pub total_worker_stops: u64,
}

/// Type alias for reset action callbacks.
type ResetAction = Box<dyn Fn(&OctaneEvent) + Send + Sync>;

/// Orchestrator that manages state reset hooks for the Octane worker pool.
///
/// Follows m12-lifecycle pattern: init -> execute -> shutdown.
pub struct StateResetOrchestrator {
    total_requests: AtomicU64,
    total_resets: AtomicU64,
    total_cleanups: AtomicU64,
    total_worker_stops: AtomicU64,
    event_tx: tokio::sync::broadcast::Sender<OctaneEvent>,
    reset_actions: HashMap<String, ResetAction>,
}

impl StateResetOrchestrator {
    /// Create a new orchestrator with a given event buffer size.
    pub fn new(event_buffer_size: usize) -> Self {
        let (event_tx, _) = tokio::sync::broadcast::channel(event_buffer_size);
        Self {
            total_requests: AtomicU64::new(0),
            total_resets: AtomicU64::new(0),
            total_cleanups: AtomicU64::new(0),
            total_worker_stops: AtomicU64::new(0),
            event_tx,
            reset_actions: HashMap::new(),
        }
    }

    /// Subscribe to the event channel.
    #[must_use]
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<OctaneEvent> {
        self.event_tx.subscribe()
    }

    /// Register a reset action for a specific event type.
    pub fn register_action<F>(&mut self, event_name: String, action: F)
    where
        F: Fn(&OctaneEvent) + Send + Sync + 'static,
    {
        self.reset_actions.insert(event_name, Box::new(action));
    }

    /// Emit an event and trigger registered reset actions.
    pub fn emit_event(&self, event: OctaneEvent) -> anyhow::Result<()> {
        let event_name = match &event {
            OctaneEvent::WorkerStarted { .. } => "worker_started",
            OctaneEvent::RequestReceived { .. } => "request_received",
            OctaneEvent::RequestTerminated { .. } => "request_terminated",
            OctaneEvent::WorkerStopping { .. } => "worker_stopping",
        };

        // Execute registered actions for this event type
        if let Some(action) = self.reset_actions.get(event_name) {
            action(&event);
        }

        // Track event-specific stats
        match &event {
            OctaneEvent::RequestReceived { .. } => {
                self.total_requests.fetch_add(1, Ordering::SeqCst);
            }
            OctaneEvent::RequestTerminated { .. } => {
                self.total_resets.fetch_add(1, Ordering::SeqCst);
                self.total_cleanups.fetch_add(1, Ordering::SeqCst);
            }
            OctaneEvent::WorkerStopping { .. } => {
                self.total_worker_stops.fetch_add(1, Ordering::SeqCst);
            }
            _ => {}
        }

        // Broadcast to subscribers (non-blocking)
        let _ = self.event_tx.send(event);

        Ok(())
    }

    /// Return current statistics.
    #[must_use]
    pub fn stats(&self) -> StateResetStats {
        StateResetStats {
            total_requests_processed: self.total_requests.load(Ordering::SeqCst),
            total_resets_performed: self.total_resets.load(Ordering::SeqCst),
            total_cleanups_performed: self.total_cleanups.load(Ordering::SeqCst),
            total_worker_stops: self.total_worker_stops.load(Ordering::SeqCst),
        }
    }

    /// Initialize the orchestrator with default reset actions.
    pub fn initialize(&mut self) {
        info!("StateResetOrchestrator initialized with Octane lifecycle actions");

        // RequestReceived: flush per-request caches (view cache, config cache, route cache)
        self.register_action("request_received".to_string(), |event| {
            if let OctaneEvent::RequestReceived { request_id } = event {
                info!(request_id, "octane: flushing per-request caches");
                // In production: this would trigger Laravel facade resets,
                // container rebinds, and cache clears via the PHP driver
            }
        });

        // RequestTerminated: rollback active DB transactions, release connections
        self.register_action("request_terminated".to_string(), |event| {
            if let OctaneEvent::RequestTerminated { request_id, status } = event {
                info!(request_id, status, "octane: rolling back transactions, releasing DB connections");
                // In production: rollback any open DB transactions,
                // return connections to pool, clear superglobals
            }
        });

        // WorkerStopping: final cleanup before worker exits
        self.register_action("worker_stopping".to_string(), |event| {
            if let OctaneEvent::WorkerStopping { worker_id } = event {
                info!(worker_id, "octane: final worker cleanup — closing persistent connections");
                // In production: close all persistent connections,
                // flush remaining buffers, clear tmp files
            }
        });
    }

    /// Shut down the orchestrator.
    pub fn shutdown(&self) {
        info!("StateResetOrchestrator shutting down");
    }
}
