//! Blue-green deployment manager for zero-downtime updates.
//! Blueprint 6 F6: Internal blue-green without Kubernetes complexity.
//!
//! Skills applied:
//! - `m07-concurrency`: ArcSwap for atomic, lock-free router switching
//! - `m12-lifecycle`: prepare → health_check → switch → drain phases
//! - `m03-mutability`: ArcSwap provides lock-free atomic state transitions
//! - `m15-anti-pattern`: No request drops during transition (atomic swap)

use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::Router;
use tracing::info;

/// Deployment slot (Blue or Green).
pub struct DeploymentSlot {
    pub name: String,
    pub router: Router,
    pub healthy: bool,
}

/// Blue-green deployment manager.
///
/// m07-concurrency: ArcSwap enables atomic router replacement without locks.
/// m12-lifecycle: Explicit phases — prepare, health_check, switch, drain.
/// m03-mutability: ArcSwap provides atomic state transitions without Mutex.
pub struct BlueGreenDeployer {
    active: ArcSwap<DeploymentSlot>,
    standby: ArcSwap<Option<DeploymentSlot>>,
    previous: ArcSwap<Option<DeploymentSlot>>,
}

impl BlueGreenDeployer {
    pub fn new(initial_router: Router) -> Self {
        let active = DeploymentSlot {
            name: "blue".to_string(),
            router: initial_router,
            healthy: true,
        };

        Self {
            active: ArcSwap::from_pointee(active),
            standby: ArcSwap::from_pointee(None),
            previous: ArcSwap::from_pointee(None),
        }
    }

    /// Prepare deployment: load new code/config into standby slot (m12-lifecycle).
    pub fn prepare_deployment(&self, name: &str, router: Router) {
        let standby = DeploymentSlot {
            name: name.to_string(),
            router,
            healthy: false,
        };
        self.standby.store(Arc::new(Some(standby)));
        info!("Deployment prepared in standby slot: {}", name);
    }

    /// Run health check on standby slot before switching (m12-lifecycle).
    pub fn health_check_standby(&self) -> bool {
        let standby = self.standby.load();
        if let Some(slot) = standby.as_ref() {
            slot.healthy
        } else {
            false
        }
    }

    /// Mark the standby slot healthy after smoke checks (CLI deploy path).
    pub fn mark_standby_healthy(&self) {
        let standby = self.standby.load();
        if let Some(slot) = standby.as_ref() {
            let updated = DeploymentSlot {
                name: slot.name.clone(),
                router: slot.router.clone(),
                healthy: true,
            };
            self.standby.store(Arc::new(Some(updated)));
            info!("Standby slot {} marked healthy", slot.name);
        }
    }

    /// Atomically switch from active to standby (m03-mutability: ArcSwap atomic).
    /// m15-anti-pattern: Zero request drops — swap is atomic, in-flight requests complete on old router.
    pub fn switch(&self) {
        let standby = self.standby.load();
        if let Some(standby_slot) = standby.as_ref() {
            let old_active = self.active.load();
            // Save old active as previous for potential rollback
            self.previous.store(Arc::new(Some(DeploymentSlot {
                name: old_active.name.clone(),
                router: old_active.router.clone(),
                healthy: old_active.healthy,
            })));

            let new_active = DeploymentSlot {
                name: standby_slot.name.clone(),
                router: standby_slot.router.clone(),
                healthy: true,
            };
            self.active.swap(Arc::new(new_active));
            info!(
                "Switched deployment: {} -> {}",
                old_active.name,
                self.active.load().name
            );
        }
    }

    /// Get the current active router (m07-concurrency: lock-free load).
    pub fn router(&self) -> Arc<Router> {
        let slot = self.active.load();
        Arc::new(slot.router.clone())
    }

    /// Rollback to previous deployment (m12-lifecycle: recovery phase).
    /// Swaps the current active slot back to previous and preserves standby.
    pub fn rollback(&self) {
        let previous = self.previous.load();
        if let Some(prev_slot) = previous.as_ref() {
            let old_active = self.active.load();
            // Only move current active to standby if standby is empty or different
            let current_standby = self.standby.load();
            let should_update_standby = current_standby.as_ref().is_none()
                || current_standby.as_ref().as_ref().map(|s| &s.name) != Some(&old_active.name);

            if should_update_standby {
                self.standby.store(Arc::new(Some(DeploymentSlot {
                    name: old_active.name.clone(),
                    router: old_active.router.clone(),
                    healthy: false,
                })));
            }
            // Restore previous as active
            let restored = DeploymentSlot {
                name: prev_slot.name.clone(),
                router: prev_slot.router.clone(),
                healthy: prev_slot.healthy,
            };
            self.active.swap(Arc::new(restored));
            info!(
                "Rolled back deployment: {} -> {}",
                old_active.name,
                self.active.load().name
            );
        } else {
            info!("No previous deployment available for rollback");
        }
    }

    /// Drain the standby slot (m12-lifecycle: final cleanup phase).
    pub fn drain_standby(&self) {
        self.standby.store(Arc::new(None));
        info!("Standby slot drained");
    }

    /// Get the current active slot (for testing/debugging).
    pub fn active(&self) -> arc_swap::Guard<Arc<DeploymentSlot>> {
        self.active.load()
    }

    /// Get the current standby slot (for testing/debugging).
    pub fn standby(&self) -> arc_swap::Guard<Arc<Option<DeploymentSlot>>> {
        self.standby.load()
    }
}

/// Serve HTTP through the active deployment slot (atomic router swap).
pub fn serving_router(deployer: Arc<BlueGreenDeployer>) -> Router {
    use axum::body::Body;
    use axum::http::Request;
    use tower::util::ServiceExt;

    Router::new().fallback(axum::routing::any_service(tower::service_fn(
        move |req: Request<Body>| {
            let deployer = deployer.clone();
            async move {
                let router = (*deployer.router()).clone();
                router.oneshot(req).await
            }
        },
    )))
}
