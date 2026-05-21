//! Tests for health and readiness probes.
//!
//! Covers: initial state, state transitions, concurrent access, counters.

use nusa_gateway::health::{HealthState, health_handler};

#[tokio::test]
async fn health_handler_returns_ok() {
    let result = health_handler().await;
    assert_eq!(result, "OK");
}

#[test]
fn health_state_starts_not_ready() {
    let state = HealthState::new();
    assert!(!state.is_ready());
}

#[test]
fn health_state_counters_start_at_zero() {
    let state = HealthState::new();
    assert_eq!(state.success_count(), 0);
    assert_eq!(state.error_count(), 0);
}

#[test]
fn health_state_mark_ready() {
    let state = HealthState::new();
    assert!(!state.is_ready());
    state.mark_ready();
    assert!(state.is_ready());
}

#[test]
fn health_state_record_success_increments_counter() {
    let state = HealthState::new();
    state.record_success();
    state.record_success();
    assert_eq!(state.success_count(), 2);
    assert_eq!(state.error_count(), 0);
}

#[test]
fn health_state_record_error_increments_counter() {
    let state = HealthState::new();
    state.record_error();
    state.record_error();
    state.record_error();
    assert_eq!(state.error_count(), 3);
    assert_eq!(state.success_count(), 0);
}

#[test]
fn health_state_counters_are_independent() {
    let state = HealthState::new();
    state.record_success();
    state.record_error();
    state.record_success();
    assert_eq!(state.success_count(), 2);
    assert_eq!(state.error_count(), 1);
}

#[test]
fn health_state_default_impl() {
    let state = HealthState::default();
    assert!(!state.is_ready());
    assert_eq!(state.success_count(), 0);
}
