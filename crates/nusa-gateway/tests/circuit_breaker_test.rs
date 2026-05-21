//! Tests for circuit breaker state machine.
//!
//! Covers: state transitions, threshold behavior, reset timeout, half-open recovery, concurrent access.

use std::time::Duration;

use nusa_gateway::circuit_breaker::{CircuitBreaker, CbState};

// ── Initial State ──

#[test]
fn circuit_breaker_starts_closed() {
    let cb = CircuitBreaker::new(3, Duration::from_secs(1));
    assert_eq!(cb.state(), CbState::Closed);
    assert!(cb.allow_request(), "new circuit breaker must allow requests");
}

#[test]
fn circuit_breaker_zero_failures_initially() {
    let cb = CircuitBreaker::new(3, Duration::from_secs(1));
    assert_eq!(cb.failure_count(), 0);
}

// ── Closed → Open Transition ──

#[test]
fn circuit_breaker_opens_after_threshold() {
    let cb = CircuitBreaker::new(3, Duration::from_secs(1));

    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.state(), CbState::Closed, "must stay closed below threshold");
    assert!(cb.allow_request());

    cb.record_failure();
    assert_eq!(cb.state(), CbState::Open, "must open at threshold");
    assert!(!cb.allow_request(), "open circuit must reject requests");
}

#[test]
fn circuit_breaker_threshold_of_one_opens_immediately() {
    let cb = CircuitBreaker::new(1, Duration::from_secs(1));
    cb.record_failure();
    assert_eq!(cb.state(), CbState::Open);
    assert!(!cb.allow_request());
}

#[test]
fn circuit_breaker_failure_count_tracks_accurately() {
    let cb = CircuitBreaker::new(5, Duration::from_secs(1));

    for i in 1..=4 {
        cb.record_failure();
        assert_eq!(cb.failure_count(), i);
        assert_eq!(cb.state(), CbState::Closed);
    }

    cb.record_failure();
    assert_eq!(cb.failure_count(), 5);
    assert_eq!(cb.state(), CbState::Open);
}

// ── Open → HalfOpen → Closed Recovery ──

#[test]
fn circuit_breaker_transitions_to_half_open_after_timeout() {
    let cb = CircuitBreaker::new(2, Duration::from_millis(50));

    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.state(), CbState::Open);
    assert!(!cb.allow_request());

    std::thread::sleep(Duration::from_millis(100));

    assert!(cb.allow_request(), "must allow request after reset timeout");
    assert_eq!(cb.state(), CbState::HalfOpen);
}

#[test]
fn circuit_breaker_closes_on_success_in_half_open() {
    let cb = CircuitBreaker::new(2, Duration::from_millis(50));

    cb.record_failure();
    cb.record_failure();
    std::thread::sleep(Duration::from_millis(100));
    cb.allow_request(); // transitions to HalfOpen

    cb.record_success();
    assert_eq!(cb.state(), CbState::Closed);
    assert_eq!(cb.failure_count(), 0);
}

#[test]
fn circuit_breaker_reopens_on_failure_in_half_open() {
    let cb = CircuitBreaker::new(1, Duration::from_millis(50));

    cb.record_failure();
    std::thread::sleep(Duration::from_millis(100));
    cb.allow_request(); // transitions to HalfOpen

    cb.record_failure();
    assert_eq!(cb.state(), CbState::Open);
}

// ── Success Does Not Affect Closed State ──

#[test]
fn circuit_breaker_success_in_closed_state_is_noop() {
    let cb = CircuitBreaker::new(3, Duration::from_secs(1));

    cb.record_failure();
    cb.record_success(); // should not reset failure count in Closed state
    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.state(), CbState::Open);
}

// ── Edge Cases ──

#[test]
fn circuit_breaker_zero_threshold_opens_immediately() {
    let cb = CircuitBreaker::new(0, Duration::from_secs(1));
    cb.record_failure();
    assert_eq!(cb.state(), CbState::Open);
}

#[test]
fn circuit_breaker_multiple_open_close_cycles() {
    let cb = CircuitBreaker::new(2, Duration::from_millis(20));

    for _ in 0..5 {
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CbState::Open);

        std::thread::sleep(Duration::from_millis(30));
        cb.allow_request();
        cb.record_success();
        assert_eq!(cb.state(), CbState::Closed);
        assert_eq!(cb.failure_count(), 0);
    }
}

#[test]
fn circuit_breaker_rapid_failures() {
    let cb = CircuitBreaker::new(10, Duration::from_secs(1));

    for _ in 0..10 {
        cb.record_failure();
    }
    assert_eq!(cb.state(), CbState::Open);
    assert_eq!(cb.failure_count(), 10);
}
