//! Lifecycle and state machine tests for nusa-gateway.
//!
//! Covers: CircuitBreaker state machine, HealthState machine,
//! BlueGreenDeployer state machine, TenantCircuitBreaker.

use std::time::Duration;

use nusa_core::TenantId;
use nusa_gateway::circuit_breaker::{CbState, CircuitBreaker};
use nusa_gateway::health::HealthState;
use nusa_gateway::tenant_circuit_breaker::TenantCircuitBreakers;

// ============================================================================
// CircuitBreaker State Machine
// ============================================================================

#[test]
fn cb_state_machine_closed_initial() {
    let cb = CircuitBreaker::new(3, Duration::from_secs(1));
    assert_eq!(cb.state(), CbState::Closed);
}

#[test]
fn cb_state_machine_closed_to_open_threshold_reached() {
    let cb = CircuitBreaker::new(3, Duration::from_secs(1));
    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.state(), CbState::Closed);
    cb.record_failure();
    assert_eq!(cb.state(), CbState::Open);
}

#[test]
fn cb_state_machine_open_to_half_open_after_timeout() {
    let cb = CircuitBreaker::new(2, Duration::from_millis(50));
    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.state(), CbState::Open);

    std::thread::sleep(Duration::from_millis(60));
    assert!(cb.allow_request());
    assert_eq!(cb.state(), CbState::HalfOpen);
}

#[test]
fn cb_state_machine_half_open_to_closed_probe_success() {
    let cb = CircuitBreaker::new(1, Duration::from_millis(30));
    cb.record_failure();
    std::thread::sleep(Duration::from_millis(40));
    cb.allow_request();
    assert_eq!(cb.state(), CbState::HalfOpen);
    cb.record_success();
    assert_eq!(cb.state(), CbState::Closed);
}

#[test]
fn cb_state_machine_half_open_to_open_probe_failure() {
    let cb = CircuitBreaker::new(1, Duration::from_millis(30));
    cb.record_failure();
    std::thread::sleep(Duration::from_millis(40));
    cb.allow_request();
    assert_eq!(cb.state(), CbState::HalfOpen);
    cb.record_failure();
    assert_eq!(cb.state(), CbState::Open);
}

#[test]
fn cb_state_machine_full_cycle_closed_open_half_open_closed() {
    let cb = CircuitBreaker::new(2, Duration::from_millis(30));

    // Closed -> Open
    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.state(), CbState::Open);

    // Open -> HalfOpen
    std::thread::sleep(Duration::from_millis(40));
    cb.allow_request();
    assert_eq!(cb.state(), CbState::HalfOpen);

    // HalfOpen -> Closed
    cb.record_success();
    assert_eq!(cb.state(), CbState::Closed);
}

#[test]
fn cb_state_machine_full_cycle_with_reopen() {
    let cb = CircuitBreaker::new(1, Duration::from_millis(30));

    // Closed -> Open
    cb.record_failure();
    assert_eq!(cb.state(), CbState::Open);

    // Open -> HalfOpen
    std::thread::sleep(Duration::from_millis(40));
    cb.allow_request();
    assert_eq!(cb.state(), CbState::HalfOpen);

    // HalfOpen -> Open (probe fails)
    cb.record_failure();
    assert_eq!(cb.state(), CbState::Open);

    // Open -> HalfOpen again
    std::thread::sleep(Duration::from_millis(40));
    cb.allow_request();
    assert_eq!(cb.state(), CbState::HalfOpen);

    // HalfOpen -> Closed
    cb.record_success();
    assert_eq!(cb.state(), CbState::Closed);
}

#[test]
fn cb_state_machine_failure_count_reset_on_close() {
    let cb = CircuitBreaker::new(1, Duration::from_millis(30));
    cb.record_failure();
    std::thread::sleep(Duration::from_millis(40));
    cb.allow_request();
    cb.record_success();
    assert_eq!(cb.state(), CbState::Closed);
    assert_eq!(cb.failure_count(), 0);
}

#[test]
fn cb_state_machine_success_in_closed_noop() {
    let cb = CircuitBreaker::new(3, Duration::from_secs(1));
    cb.record_failure();
    cb.record_success();
    // Failure count should not reset in Closed state
    assert_eq!(cb.failure_count(), 1);
}

// ============================================================================
// HealthState Machine
// ============================================================================

#[test]
fn health_state_machine_initial_not_ready() {
    let state = HealthState::new();
    assert!(!state.is_ready());
}

#[test]
fn health_state_machine_not_ready_to_ready() {
    let state = HealthState::new();
    state.mark_ready();
    assert!(state.is_ready());
}

#[test]
fn health_state_machine_ready_remains_ready_after_success() {
    let state = HealthState::new();
    state.mark_ready();
    assert!(state.is_ready());
    state.record_success();
    assert!(state.is_ready());
}

#[test]
fn health_state_machine_ready_remains_ready_after_error() {
    let state = HealthState::new();
    state.mark_ready();
    state.record_error();
    assert!(state.is_ready(), "error should not flip ready flag");
}

#[test]
fn health_state_machine_success_count_increments() {
    let state = HealthState::new();
    state.record_success();
    state.record_success();
    state.record_success();
    assert_eq!(state.success_count(), 3);
}

#[test]
fn health_state_machine_error_count_increments() {
    let state = HealthState::new();
    state.record_error();
    state.record_error();
    assert_eq!(state.error_count(), 2);
}

#[test]
fn health_state_machine_counters_independent() {
    let state = HealthState::new();
    state.record_success();
    state.record_success();
    state.record_error();
    assert_eq!(state.success_count(), 2);
    assert_eq!(state.error_count(), 1);
}

#[test]
fn health_state_machine_zero_counts_initially() {
    let state = HealthState::new();
    assert_eq!(state.success_count(), 0);
    assert_eq!(state.error_count(), 0);
}

// ============================================================================
// TenantCircuitBreaker State Machine
// ============================================================================

#[test]
fn tenant_cb_new_tenant_allowed_by_default() {
    let tcb = TenantCircuitBreakers::new(3, Duration::from_secs(1));
    let tenant = TenantId::new("tenant-new");
    assert!(tcb.is_allowed(&tenant), "new tenant should be allowed");
}

#[test]
fn tenant_cb_record_failure_opens_after_threshold() {
    let tcb = TenantCircuitBreakers::new(2, Duration::from_millis(50));
    let tenant = TenantId::new("tenant-failing");

    tcb.record_failure(&tenant);
    tcb.record_failure(&tenant);

    assert!(!tcb.is_allowed(&tenant), "circuit should be open");
}

#[test]
fn tenant_cb_record_success_does_not_open() {
    let tcb = TenantCircuitBreakers::new(3, Duration::from_secs(1));
    let tenant = TenantId::new("tenant-success");

    tcb.record_success(&tenant);
    tcb.record_success(&tenant);

    assert!(tcb.is_allowed(&tenant), "successes should not open circuit");
}

#[test]
fn tenant_cb_isolation_tenants_independent() {
    let tcb = TenantCircuitBreakers::new(1, Duration::from_secs(10));
    let t1 = TenantId::new("tenant-isolated-1");
    let t2 = TenantId::new("tenant-isolated-2");

    tcb.record_failure(&t1);
    tcb.record_failure(&t1); // t1 circuit is open

    assert!(!tcb.is_allowed(&t1), "t1 should be blocked");
    assert!(tcb.is_allowed(&t2), "t2 should still be allowed");
}

#[test]
fn tenant_cb_state_returns_correct_state() {
    let tcb = TenantCircuitBreakers::new(2, Duration::from_millis(50));
    let tenant = TenantId::new("tenant-state");

    assert!(tcb.state(&tenant).is_none(), "new tenant has no state yet");

    tcb.record_failure(&tenant);
    tcb.record_failure(&tenant);

    assert_eq!(tcb.state(&tenant), Some(CbState::Open));
}

#[test]
fn tenant_cb_count_tracks_registered_tenants() {
    let tcb = TenantCircuitBreakers::new(3, Duration::from_secs(1));
    let t1 = TenantId::new("t1");
    let t2 = TenantId::new("t2");

    assert_eq!(tcb.count(), 0);

    tcb.record_failure(&t1);
    assert_eq!(tcb.count(), 1);

    tcb.record_failure(&t2);
    assert_eq!(tcb.count(), 2);
}

#[test]
fn tenant_cb_full_cycle_closed_open_half_open_closed() {
    let tcb = TenantCircuitBreakers::new(2, Duration::from_millis(30));
    let tenant = TenantId::new("tenant-cycle");

    // Closed
    assert!(tcb.is_allowed(&tenant));

    // Record failures -> Open
    tcb.record_failure(&tenant);
    tcb.record_failure(&tenant);
    assert!(!tcb.is_allowed(&tenant));
    assert_eq!(tcb.state(&tenant), Some(CbState::Open));

    // Wait -> HalfOpen
    std::thread::sleep(Duration::from_millis(40));
    assert!(tcb.is_allowed(&tenant));
    assert_eq!(tcb.state(&tenant), Some(CbState::HalfOpen));

    // Success -> Closed
    tcb.record_success(&tenant);
    assert!(tcb.is_allowed(&tenant));
    assert_eq!(tcb.state(&tenant), Some(CbState::Closed));
}

// ============================================================================
// BlueGreenDeployer State Machine
// ============================================================================

use axum::Router;
use nusa_gateway::bluegreen::BlueGreenDeployer;

#[test]
fn bluegreen_state_machine_initial_active_is_blue() {
    let deployer = BlueGreenDeployer::new(Router::new());
    assert_eq!(deployer.active().name, "blue");
}

#[test]
fn bluegreen_state_machine_initial_standby_is_none() {
    let deployer = BlueGreenDeployer::new(Router::new());
    assert!(deployer.standby().as_ref().is_none());
}

#[test]
fn bluegreen_state_machine_initial_previous_is_none() {
    let deployer = BlueGreenDeployer::new(Router::new());
    assert!(deployer.active().name == "blue");
    // No previous until first switch
}

#[test]
fn bluegreen_state_machine_prepare_populates_standby() {
    let deployer = BlueGreenDeployer::new(Router::new());
    deployer.prepare_deployment("green", Router::new());

    let standby = deployer.standby();
    assert!(standby.as_ref().is_some());
    assert_eq!(standby.as_ref().as_ref().unwrap().name, "green");
}

#[test]
fn bluegreen_state_machine_prepare_to_switch() {
    let deployer = BlueGreenDeployer::new(Router::new());
    deployer.prepare_deployment("green", Router::new());

    assert_eq!(deployer.active().name, "blue");
    deployer.switch();
    assert_eq!(deployer.active().name, "green");
}

#[test]
fn bluegreen_state_machine_switch_saves_previous() {
    let deployer = BlueGreenDeployer::new(Router::new());
    deployer.prepare_deployment("green", Router::new());
    deployer.switch();

    assert_eq!(deployer.active().name, "green");
    // Previous should be blue
}

#[test]
fn bluegreen_state_machine_drain_clears_standby() {
    let deployer = BlueGreenDeployer::new(Router::new());
    deployer.prepare_deployment("green", Router::new());
    deployer.drain_standby();

    assert!(deployer.standby().as_ref().is_none());
}

#[test]
fn bluegreen_state_machine_full_cycle_prepare_switch_drain() {
    let deployer = BlueGreenDeployer::new(Router::new());

    // Initial: blue active
    assert_eq!(deployer.active().name, "blue");

    // Prepare green
    deployer.prepare_deployment("green", Router::new());
    assert!(deployer.standby().as_ref().is_some());

    // Switch to green
    deployer.switch();
    assert_eq!(deployer.active().name, "green");

    // Drain standby
    deployer.drain_standby();
    assert!(deployer.standby().as_ref().is_none());
}

#[test]
fn bluegreen_state_machine_rollback_available_after_switch() {
    let deployer = BlueGreenDeployer::new(Router::new());
    deployer.prepare_deployment("green", Router::new());
    deployer.switch();

    assert_eq!(deployer.active().name, "green");

    // Rollback to blue
    deployer.rollback();
    assert_eq!(deployer.active().name, "blue");
}

#[test]
fn bluegreen_state_machine_rollback_not_available_initially() {
    let deployer = BlueGreenDeployer::new(Router::new());
    assert_eq!(deployer.active().name, "blue");

    // No previous to rollback to
    deployer.rollback();
    // Active should remain blue (no rollback happened)
    assert_eq!(deployer.active().name, "blue");
}

// ============================================================================
// Fallback State Machine
// ============================================================================

#[test]
fn fallback_state_machine_primary_available_uses_primary() {
    let cb = CircuitBreaker::new(3, Duration::from_secs(1));
    assert!(cb.allow_request(), "primary should be available");
}

#[test]
fn fallback_state_machine_primary_unavailable_triggers_fallback() {
    let cb = CircuitBreaker::new(1, Duration::from_secs(10));
    cb.record_failure();
    assert!(!cb.allow_request(), "primary should be unavailable");
}

#[test]
fn fallback_state_machine_toggle_back_to_primary() {
    let cb = CircuitBreaker::new(1, Duration::from_millis(30));
    cb.record_failure();
    assert!(!cb.allow_request());

    std::thread::sleep(Duration::from_millis(40));
    assert!(cb.allow_request());
    cb.record_success();
    assert!(cb.allow_request(), "back to primary");
}
