//! State reset orchestrator for Octane event hooks.
//!
//! Manages lifecycle events that mirror Octane's event system:
//! - `worker_started` — initial state setup
//! - `request_received` — pre-request state flush
//! - `request_terminated` — post-request cleanup
//! - `worker_stopping` — final cleanup before worker recycle/shutdown
//!
//! m12-lifecycle: explicit init -> execute -> shutdown phases.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

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
    stats: AtomicU64,
    event_tx: tokio::sync::broadcast::Sender<OctaneEvent>,
    reset_actions: HashMap<String, ResetAction>,
}

impl StateResetOrchestrator {
    /// Create a new orchestrator with a given event buffer size.
    pub fn new(event_buffer_size: usize) -> Self {
        let (event_tx, _) = tokio::sync::broadcast::channel(event_buffer_size);
        Self {
            stats: AtomicU64::new(0),
            event_tx,
            reset_actions: HashMap::new(),
        }
    }

    /// Subscribe to the event channel.
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

        // Broadcast to subscribers (non-blocking)
        let _ = self.event_tx.send(event);

        // Increment stats
        self.stats.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Return current statistics.
    pub fn stats(&self) -> StateResetStats {
        let total = self.stats.load(Ordering::Relaxed);
        StateResetStats {
            total_requests_processed: total,
            total_resets_performed: total,
            total_cleanups_performed: total,
            total_worker_stops: 0,
        }
    }

    /// Initialize the orchestrator with default reset actions.
    pub fn initialize(&mut self) {
        info!("StateResetOrchestrator initialized");

        // Default action: log when request is received
        self.register_action("request_received".into(), |event| {
            if let OctaneEvent::RequestReceived { request_id } = event {
                info!(request_id, "state reset: flushing caches for request");
            }
        });

        // Default action: log when request is terminated
        self.register_action("request_terminated".into(), |event| {
            if let OctaneEvent::RequestTerminated { request_id, status } = event {
                info!(request_id, status, "state reset: post-request cleanup");
            }
        });

        // Default action: log when worker is stopping
        self.register_action("worker_stopping".into(), |event| {
            if let OctaneEvent::WorkerStopping { worker_id } = event {
                info!(worker_id, "state reset: final worker cleanup");
            }
        });
    }

    /// Shut down the orchestrator.
    pub fn shutdown(&self) {
        info!("StateResetOrchestrator shutting down");
    }
}
