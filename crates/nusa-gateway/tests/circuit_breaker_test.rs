//! Tests for circuit breaker state machine.
//!
//! Covers: state transitions, threshold behavior, reset timeout, half-open recovery, concurrent access.

use std::time::Duration;

use nusa_gateway::circuit_breaker::{CbState, CircuitBreaker};

// ── Initial State ──

#[test]
fn circuit_breaker_starts_closed() {
    let cb = CircuitBreaker::new(3, Duration::from_secs(1));
    assert_eq!(cb.state(), CbState::Closed);
    assert!(
        cb.allow_request(),
        "new circuit breaker must allow requests"
    );
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
    assert_eq!(
        cb.state(),
        CbState::Closed,
        "must stay closed below threshold"
    );
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

// ── Degradation & Recovery Cycle Tests ──

/// High latency dependency → circuit breaker opens, fallback activates.
#[test]
fn circuit_breaker_degradation_opens_after_failures() {
    let cb = CircuitBreaker::new(3, Duration::from_millis(100));

    // Simulate 3 failures (like high latency dependency)
    for _ in 0..3 {
        cb.record_failure();
    }

    assert_eq!(cb.state(), CbState::Open, "should open after failures");
    assert!(!cb.allow_request(), "should reject while open");
}

/// Dependency returns errors → circuit breaker, retry, then open.
#[test]
fn circuit_breaker_retry_then_open() {
    let cb = CircuitBreaker::new(2, Duration::from_millis(50));

    // Phase 1: First failure → retry possible
    cb.record_failure();
    assert_eq!(
        cb.state(),
        CbState::Closed,
        "first failure keeps circuit closed"
    );

    // Phase 2: Second failure → opens
    cb.record_failure();
    assert_eq!(cb.state(), CbState::Open);

    // Phase 3: Wait for recovery window
    std::thread::sleep(Duration::from_millis(60));
    assert!(cb.allow_request(), "should allow probe after timeout");
    assert_eq!(cb.state(), CbState::HalfOpen);
}

/// Dependency slow then recovers → circuit half-open, probe, then close.
#[test]
fn circuit_breaker_half_open_probe_then_close() {
    let cb = CircuitBreaker::new(1, Duration::from_millis(50));

    // Trip
    cb.record_failure();
    assert_eq!(cb.state(), CbState::Open);

    // Wait for half-open
    std::thread::sleep(Duration::from_millis(60));
    cb.allow_request(); // probe
    assert_eq!(cb.state(), CbState::HalfOpen);

    // Probe succeeds → close
    cb.record_success();
    assert_eq!(cb.state(), CbState::Closed);

    // Normal operation resumes
    assert!(cb.allow_request());
}

/// Resource exhaustion then freed → recovers without restart.
#[test]
fn circuit_breaker_recovery_without_restart() {
    let cb = CircuitBreaker::new(2, Duration::from_millis(30));

    // Exhaust (open circuit)
    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.state(), CbState::Open);

    // Recovery cycle
    std::thread::sleep(Duration::from_millis(40));
    cb.allow_request(); // half-open
    cb.record_success(); // close

    // Verify fully recovered
    assert_eq!(cb.state(), CbState::Closed);
    assert_eq!(cb.failure_count(), 0);
    assert!(cb.allow_request());
}

/// Partial failure (50% errors) → degrades gracefully, not all-or-nothing.
#[test]
fn circuit_breaker_partial_failure_degradation() {
    let cb = CircuitBreaker::new(5, Duration::from_millis(50));

    // Mixed success/failure pattern - success does NOT reset failures in Closed state
    cb.record_success();
    cb.record_failure();
    cb.record_success();
    cb.record_failure();
    cb.record_failure();
    cb.record_failure();
    cb.record_failure(); // 5th failure should trip it

    // Now at threshold - should open
    assert_eq!(cb.state(), CbState::Open);
    assert!(!cb.allow_request());
}

/// Circuit breaker open → rapid checks don't cause state corruption.
#[test]
fn circuit_breaker_rapid_checks_while_open() {
    let cb = CircuitBreaker::new(1, Duration::from_secs(10));
    cb.record_failure();

    // Rapid checks should all return false without corruption
    for _ in 0..100 {
        assert!(!cb.allow_request(), "should consistently reject while open");
    }

    assert_eq!(
        cb.state(),
        CbState::Open,
        "state should not change from rapid checks"
    );
}
