//! Comprehensive tests for BlueGreenDeployer.
//!
//! Covers: prepare, switch, rollback, drain, concurrent safety,
//! state transitions, and edge cases.

use axum::Router;
use nusa_gateway::bluegreen::BlueGreenDeployer;

// ── Initial State ──

#[test]
fn bluegreen_starts_with_active_slot() {
    let deployer = BlueGreenDeployer::new(Router::new());
    // Should not panic — active slot is always available
    let _router = deployer.router();
}

#[test]
fn bluegreen_initial_slot_is_named_blue() {
    let deployer = BlueGreenDeployer::new(Router::new());
    let active = deployer.active();
    assert_eq!(active.name, "blue");
}

#[test]
fn bluegreen_initial_slot_is_healthy() {
    let deployer = BlueGreenDeployer::new(Router::new());
    assert!(deployer.active().healthy);
}

#[test]
fn bluegreen_standby_is_empty_initially() {
    let deployer = BlueGreenDeployer::new(Router::new());
    assert!(!deployer.health_check_standby());
}

// ── Prepare Deployment ──

#[test]
fn bluegreen_prepare_creates_standby() {
    let deployer = BlueGreenDeployer::new(Router::new());
    deployer.prepare_deployment("green", Router::new());
    // Standby should now exist but be unhealthy
    assert!(!deployer.health_check_standby());
}

#[test]
fn bluegreen_prepare_sets_standby_name() {
    let deployer = BlueGreenDeployer::new(Router::new());
    deployer.prepare_deployment("green-v2", Router::new());
    let standby = deployer.standby();
    assert!(standby.is_some());
    assert_eq!(standby.as_ref().as_ref().unwrap().name, "green-v2");
}

#[test]
fn bluegreen_prepare_marks_standby_unhealthy() {
    let deployer = BlueGreenDeployer::new(Router::new());
    deployer.prepare_deployment("green", Router::new());
    let standby = deployer.standby();
    assert!(standby.is_some());
    assert!(!standby.as_ref().as_ref().unwrap().healthy);
}

#[test]
fn bluegreen_prepare_overwrites_previous_standby() {
    let deployer = BlueGreenDeployer::new(Router::new());
    deployer.prepare_deployment("first", Router::new());
    deployer.prepare_deployment("second", Router::new());
    let standby = deployer.standby();
    assert_eq!(standby.as_ref().as_ref().unwrap().name, "second");
}

// ── Switch Deployment ──

#[test]
fn bluegreen_mark_standby_healthy_enables_health_check() {
    let deployer = BlueGreenDeployer::new(Router::new());
    deployer.prepare_deployment("green", Router::new());
    assert!(!deployer.health_check_standby());
    deployer.mark_standby_healthy();
    assert!(deployer.health_check_standby());
}

#[test]
fn bluegreen_switch_marks_new_active_healthy() {
    let deployer = BlueGreenDeployer::new(Router::new());
    deployer.prepare_deployment("green", Router::new());
    deployer.mark_standby_healthy();
    deployer.switch();
    assert!(deployer.active().healthy);
    assert_eq!(deployer.active().name, "green");
}

#[test]
fn bluegreen_switch_without_standby_is_noop() {
    let deployer = BlueGreenDeployer::new(Router::new());
    let before = deployer.active().name.clone();
    deployer.switch();
    assert_eq!(deployer.active().name, before);
}

// ── Rollback ──

#[test]
fn bluegreen_rollback_without_previous_is_noop() {
    let deployer = BlueGreenDeployer::new(Router::new());
    let before = deployer.active().name.clone();
    deployer.rollback();
    // Should not panic, just log
    assert_eq!(deployer.active().name, before);
}

#[test]
fn bluegreen_rollback_restores_previous() {
    let deployer = BlueGreenDeployer::new(Router::new());
    // Prepare and switch to green
    deployer.prepare_deployment("green", Router::new());
    deployer.switch();
    assert_eq!(deployer.active().name, "green");

    // Rollback should restore blue
    deployer.rollback();
    assert_eq!(deployer.active().name, "blue");
}

#[test]
fn bluegreen_rollback_moves_current_to_standby() {
    let deployer = BlueGreenDeployer::new(Router::new());
    deployer.prepare_deployment("green", Router::new());
    deployer.switch();

    // After rollback, the current (green) should be in standby
    deployer.rollback();
    let standby = deployer.standby();
    assert!(standby.is_some());
    assert_eq!(standby.as_ref().as_ref().unwrap().name, "green");
}

#[test]
fn bluegreen_multiple_switches_and_rollbacks() {
    let deployer = BlueGreenDeployer::new(Router::new());

    // First deployment: blue -> v2
    deployer.prepare_deployment("v2", Router::new());
    deployer.switch();
    assert_eq!(deployer.active().name, "v2");

    // Rollback: v2 -> blue (previous is blue)
    deployer.rollback();
    assert_eq!(deployer.active().name, "blue");

    // Second deployment: blue -> v3
    deployer.prepare_deployment("v3", Router::new());
    deployer.switch();
    assert_eq!(deployer.active().name, "v3");

    // Rollback: v3 -> blue (previous is still blue, since blue was active before switch to v3)
    deployer.rollback();
    assert_eq!(deployer.active().name, "blue");
}

#[test]
fn bluegreen_rollback_moves_active_to_standby() {
    let deployer = BlueGreenDeployer::new(Router::new());
    deployer.prepare_deployment("green", Router::new());
    deployer.switch();

    // After switch: active=green, previous=blue, standby=None (consumed by switch)
    // Prepare a new standby before rollback
    deployer.prepare_deployment("v3", Router::new());
    // Now: active=green, previous=blue, standby=v3
    deployer.rollback();
    // After rollback: active=blue, previous=green, standby=green (old active replaces standby)
    // Rollback moves the old active (green) to standby as the new "previous"

    let standby = deployer.standby();
    assert!(standby.is_some());
    // Standby is the rolled-back deployment (green)
    assert_eq!(standby.as_ref().as_ref().unwrap().name, "green");
}

// ── Drain ──

#[test]
fn bluegreen_drain_clears_standby() {
    let deployer = BlueGreenDeployer::new(Router::new());
    deployer.prepare_deployment("green", Router::new());
    assert!(deployer.standby().is_some());

    deployer.drain_standby();
    assert!(deployer.standby().is_none());
}

#[test]
fn bluegreen_drain_empty_standby_is_noop() {
    let deployer = BlueGreenDeployer::new(Router::new());
    deployer.drain_standby(); // should not panic
}

// ── Concurrent Safety (m07-concurrency) ──

#[test]
fn bluegreen_concurrent_prepare_and_switch() {
    use std::sync::Arc;
    use std::thread;

    let deployer = Arc::new(BlueGreenDeployer::new(Router::new()));

    let d1 = deployer.clone();
    let t1 = thread::spawn(move || {
        d1.prepare_deployment("concurrent-green", Router::new());
        d1.switch();
    });

    let d2 = deployer.clone();
    let t2 = thread::spawn(move || {
        d2.prepare_deployment("concurrent-blue", Router::new());
    });

    t1.join().expect("thread must not panic");
    t2.join().expect("thread must not panic");

    // Should be in a valid state
    let _ = deployer.router();
}

#[test]
fn bluegreen_concurrent_rollbacks() {
    use std::sync::Arc;
    use std::thread;

    let deployer = Arc::new(BlueGreenDeployer::new(Router::new()));
    deployer.prepare_deployment("green", Router::new());
    deployer.switch();

    let mut handles = vec![];
    for _ in 0..5 {
        let d = deployer.clone();
        handles.push(thread::spawn(move || {
            d.rollback();
        }));
    }

    for h in handles {
        h.join().expect("thread must not panic");
    }

    // Should be in a valid state
    let _ = deployer.active();
}

// ── Edge Cases ──

#[test]
fn bluegreen_switch_then_drain() {
    let deployer = BlueGreenDeployer::new(Router::new());
    deployer.prepare_deployment("green", Router::new());
    deployer.switch();
    deployer.drain_standby();

    // After drain, standby is cleared but active remains
    assert_eq!(deployer.active().name, "green");
    assert!(deployer.standby().is_none());
}
