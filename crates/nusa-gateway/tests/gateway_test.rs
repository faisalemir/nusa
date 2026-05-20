//! Integration tests for nusa-gateway crate
//!
//! Skills applied:
//! - `m07-concurrency`: AtomicU64 for counters, thread-safety testing
//! - `m13-domain-error`: Circuit breaker state machine coverage
//! - `coding-guidelines`: assert! with descriptive messages

use nusa_gateway::circuit_breaker::CircuitBreaker;
use std::time::Duration;

// ── Initial State ──

#[test]
fn circuit_breaker_starts_closed() {
    let cb = CircuitBreaker::new(3, Duration::from_secs(1));
    assert!(cb.allow_request(), "new circuit breaker must be closed");
}

// ── Closed → Open Transition ──

#[test]
fn circuit_breaker_opens_after_threshold_reached() {
    let cb = CircuitBreaker::new(3, Duration::from_millis(100));

    // Record failures up to threshold - 1
    cb.record_failure();
    cb.record_failure();
    assert!(cb.allow_request(), "must still allow requests below threshold");

    // Record the final failure that trips the breaker
    cb.record_failure();
    assert!(!cb.allow_request(), "must reject requests after threshold");
}

#[test]
fn circuit_breaker_threshold_is_exact() {
    let threshold = 5;
    let cb = CircuitBreaker::new(threshold, Duration::from_secs(1));

    for _ in 0..threshold - 1 {
        cb.record_failure();
    }
    assert!(cb.allow_request(), "must allow at threshold - 1 failures");

    cb.record_failure();
    assert!(!cb.allow_request(), "must reject at threshold failures");
}

// ── Open → HalfOpen Transition ──

#[test]
fn circuit_breaker_transitions_to_half_open_after_timeout() {
    let cb = CircuitBreaker::new(1, Duration::from_millis(50));

    cb.record_failure();
    assert!(!cb.allow_request(), "must be open immediately after failure");

    std::thread::sleep(Duration::from_millis(60));
    assert!(cb.allow_request(), "must transition to half-open after timeout");
}

#[test]
fn circuit_breaker_stays_open_during_timeout() {
    let cb = CircuitBreaker::new(1, Duration::from_secs(10));

    cb.record_failure();
    assert!(!cb.allow_request(), "must be open");
    assert!(!cb.allow_request(), "must stay open on repeated checks");
}

// ── HalfOpen → Closed Transition ──

#[test]
fn circuit_breaker_closes_on_success_from_half_open() {
    let cb = CircuitBreaker::new(1, Duration::from_millis(50));

    cb.record_failure();
    std::thread::sleep(Duration::from_millis(60));
    cb.allow_request(); // transitions to half-open
    cb.record_success(); // should close

    assert!(cb.allow_request(), "must be closed after success from half-open");
}

#[test]
fn circuit_breaker_failure_from_half_open_reopens() {
    let cb = CircuitBreaker::new(1, Duration::from_millis(50));

    cb.record_failure();
    std::thread::sleep(Duration::from_millis(60));
    cb.allow_request(); // transitions to half-open
    cb.record_failure(); // should reopen

    assert!(!cb.allow_request(), "must reopen after failure from half-open");
}

// ── Failure Count Reset ──

#[test]
fn circuit_breaker_resets_failure_count_on_half_open_success() {
    let cb = CircuitBreaker::new(3, Duration::from_millis(50));

    // Trip the breaker
    cb.record_failure();
    cb.record_failure();
    cb.record_failure();
    assert!(!cb.allow_request(), "must be open");

    // Wait for half-open
    std::thread::sleep(Duration::from_millis(60));
    cb.allow_request();
    cb.record_success();

    // Can we accumulate failures again without immediately tripping?
    cb.record_failure();
    cb.record_failure();
    assert!(
        cb.allow_request(),
        "failure count must have been reset — 2 < threshold of 3",
    );
}

// ── Concurrent Safety (m07-concurrency) ──

#[test]
fn circuit_breaker_thread_safety() {
    use std::sync::Arc;
    use std::thread;

    let cb = Arc::new(CircuitBreaker::new(100, Duration::from_secs(10)));
    let mut handles = vec![];

    // Spawn multiple threads recording failures concurrently
    for _ in 0..10 {
        let cb = Arc::clone(&cb);
        handles.push(thread::spawn(move || {
            for _ in 0..20 {
                cb.record_failure();
            }
        }));
    }

    for h in handles {
        h.join().expect("thread must not panic");
    }

    // After all threads complete, the breaker must be open (10 * 20 = 200 > 100)
    assert!(
        !cb.allow_request(),
        "circuit breaker must be open after concurrent failures",
    );
}
