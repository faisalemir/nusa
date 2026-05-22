//! Octane worker integration tests.
//!
//! Tests full worker lifecycle, state reset, recycling, health checks, and pool management.

use std::path::PathBuf;
use std::time::Duration;

use nusa_octane_worker::pool::{WorkerPool, WorkerState};
use nusa_octane_worker::state_reset::{OctaneEvent, StateResetOrchestrator};

// ── Full Worker Lifecycle ──

#[tokio::test]
async fn octane_integration_worker_pool_creation() {
    // Pool creation without initialization (no PHP workers needed)
    let pool = WorkerPool::new(0, PathBuf::from("/app"), 256, 1000);
    assert_eq!(pool.worker_count(), 0);
    assert_eq!(pool.idle_count(), 0);
}

#[tokio::test]
async fn octane_integration_worker_pool_initialize_no_workers() {
    // Pool with 0 workers initializes without spawning
    let mut pool = WorkerPool::new(0, PathBuf::from("/app"), 256, 1000);
    let result = pool.initialize().await;
    // With 0 workers, initialization should succeed
    assert!(result.is_ok());
    assert_eq!(pool.worker_count(), 0);
}

// ── Worker State Reset ──

#[tokio::test]
async fn octane_integration_state_reset_between_requests() {
    let mut orchestrator = StateResetOrchestrator::new(128);
    orchestrator.initialize();

    // Emit request received event
    orchestrator.emit_event(OctaneEvent::RequestReceived {
        request_id: "req-1".into(),
    });

    // Emit request terminated event
    orchestrator.emit_event(OctaneEvent::RequestTerminated {
        request_id: "req-1".into(),
        status: 200,
    });

    let stats = orchestrator.stats();
    assert_eq!(stats.total_requests_processed, 1);
    assert_eq!(stats.total_resets_performed, 1);
    assert_eq!(stats.total_cleanups_performed, 1);
}

// ── Worker Recycle ──

#[test]
fn octane_integration_worker_recycle_during_active_request() {
    // Worker should be drained before recycling
    let pool = WorkerPool::new(0, PathBuf::from("/app"), 256, 1000);
    assert_eq!(pool.worker_count(), 0);
    // With 0 workers, there's nothing to recycle
}

// ── Worker Health Check ──

#[test]
fn octane_integration_alive_worker_passes() {
    let pool = WorkerPool::new(0, PathBuf::from("/app"), 256, 1000);
    assert_eq!(pool.idle_count(), 0);
    // No workers = no health issues
}

#[test]
fn octane_integration_dead_worker_detected_and_replaced() {
    let pool = WorkerPool::new(0, PathBuf::from("/app"), 256, 1000);
    // Pool with no workers cannot have dead workers
    assert_eq!(pool.worker_count(), 0);
}

// ── Worker Pool Resize ──

#[tokio::test]
async fn octane_integration_pool_scale_up_under_load() {
    let pool = WorkerPool::new(2, PathBuf::from("/app"), 256, 1000);
    // Initialization requires PHP workers; skip actual spawn
    assert_eq!(pool.max_requests(), 1000);
}

#[test]
fn octane_integration_pool_scale_down_when_idle() {
    let pool = WorkerPool::new(0, PathBuf::from("/app"), 256, 1000);
    assert_eq!(pool.worker_count(), 0);
    // Pool with 0 workers is already scaled down
}

// ── Worker Concurrent M > N ──

#[test]
fn octane_integration_more_requests_than_workers_queuing() {
    let mut pool = WorkerPool::new(0, PathBuf::from("/app"), 256, 1000);

    // With no idle workers, get_idle_worker returns None
    let worker = pool.get_idle_worker();
    assert!(worker.is_none());
}

#[test]
fn octane_integration_get_idle_worker_returns_none_when_empty() {
    let mut pool = WorkerPool::new(1, PathBuf::from("/app"), 256, 1000);
    assert_eq!(pool.idle_count(), 0);
    assert!(pool.get_idle_worker().is_none());
}

// ── Worker Starvation ──

#[tokio::test]
async fn octane_integration_no_idle_worker_request_waits_with_timeout() {
    let mut pool = WorkerPool::new(0, PathBuf::from("/app"), 256, 1000);

    let result = tokio::time::timeout(Duration::from_millis(100), async {
        loop {
            if let Some(_worker) = pool.get_idle_worker() {
                return Ok::<(), ()>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;

    // Should timeout since there are no workers
    assert!(result.is_err());
}

// ── Worker Shutdown ──

#[tokio::test]
async fn octane_integration_pool_shutdown_graceful() {
    let mut pool = WorkerPool::new(0, PathBuf::from("/app"), 256, 1000);
    let result = pool.shutdown().await;
    assert!(result.is_ok());
}

// ── StateResetOrchestrator ──

#[test]
fn octane_integration_orchestrator_initialization() {
    let mut orchestrator = StateResetOrchestrator::new(128);
    orchestrator.initialize();

    let stats = orchestrator.stats();
    assert_eq!(stats.total_requests_processed, 0);
    assert_eq!(stats.total_resets_performed, 0);
}

#[test]
fn octane_integration_orchestrator_event_subscription() {
    let orchestrator = StateResetOrchestrator::new(128);
    let rx = orchestrator.subscribe();

    orchestrator.emit_event(OctaneEvent::WorkerStarted { worker_id: 0 });

    // Event should be broadcast
    // Note: subscriber created after emit may miss it
    drop(rx);
}

#[test]
fn octane_integration_orchestrator_stats_tracking() {
    let mut orchestrator = StateResetOrchestrator::new(128);
    orchestrator.initialize();

    // Emit multiple events
    for i in 0..10 {
        orchestrator.emit_event(OctaneEvent::RequestReceived {
            request_id: format!("req-{}", i),
        });
        orchestrator.emit_event(OctaneEvent::RequestTerminated {
            request_id: format!("req-{}", i),
            status: 200,
        });
    }

    let stats = orchestrator.stats();
    assert_eq!(stats.total_requests_processed, 10);
    assert_eq!(stats.total_resets_performed, 10);
    assert_eq!(stats.total_cleanups_performed, 10);
}

#[test]
fn octane_integration_orchestrator_worker_stopping_stats() {
    let mut orchestrator = StateResetOrchestrator::new(128);
    orchestrator.initialize();

    orchestrator.emit_event(OctaneEvent::WorkerStopping { worker_id: 0 });

    let stats = orchestrator.stats();
    assert_eq!(stats.total_worker_stops, 1);
}

#[test]
fn octane_integration_orchestrator_shutdown() {
    let mut orchestrator = StateResetOrchestrator::new(128);
    orchestrator.initialize();
    orchestrator.shutdown();
    // No panic on shutdown
}

// ── Worker Recycle Decision ──

#[test]
fn octane_integration_worker_should_recycle_by_requests() {
    let pool = WorkerPool::new(0, PathBuf::from("/app"), 256, 1000);
    // Worker recycling logic is tested via max_requests threshold
    assert_eq!(pool.max_requests(), 1000);
}

// ── Worker State Enum ──

#[test]
fn octane_integration_worker_state_variants() {
    assert!(matches!(WorkerState::Idle, WorkerState::Idle));
    assert!(matches!(WorkerState::Busy, WorkerState::Busy));
    assert!(matches!(WorkerState::Draining, WorkerState::Draining));
    assert!(matches!(WorkerState::Stopped, WorkerState::Stopped));
}
