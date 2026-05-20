//! Circuit breaker state machine for traffic protection.
//!
//! Skills applied:
//! - `m13-domain-error`: Protects against cascading failures
//! - `m07-concurrency`: Single Mutex prevents race conditions
//! - `m15-anti-pattern`: All state mutations under one lock

use std::time::{Duration, Instant};

use parking_lot::Mutex;

/// Circuit Breaker state machine (m13-domain-error)
///
/// Protects against cascading failures during traffic spikes or worker crashes.
/// m07-concurrency: All state mutations are protected by a single Mutex to
/// prevent race conditions between concurrent threads.
///
/// # Example
/// ```rust
/// use nusa_gateway::circuit_breaker::CircuitBreaker;
/// use std::time::Duration;
///
/// // Create circuit breaker: opens after 3 failures, resets after 1 second
/// let cb = CircuitBreaker::new(3, Duration::from_secs(1));
///
/// // Initially closed — requests are allowed
/// assert!(cb.allow_request());
///
/// // Record failures until threshold
/// cb.record_failure();
/// cb.record_failure();
/// cb.record_failure();
///
/// // Now open — requests are rejected
/// assert!(!cb.allow_request());
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CbState {
    Closed,
    Open,
    HalfOpen,
}

struct CircuitBreakerState {
    cb_state: CbState,
    failure_count: u64,
    last_failure_time: Option<Instant>,
}

pub struct CircuitBreaker {
    state: Mutex<CircuitBreakerState>,
    failure_threshold: u64,
    reset_timeout: Duration,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u64, reset_timeout: Duration) -> Self {
        Self {
            state: Mutex::new(CircuitBreakerState {
                cb_state: CbState::Closed,
                failure_count: 0,
                last_failure_time: None,
            }),
            failure_threshold,
            reset_timeout,
        }
    }

    /// Check if a request should be allowed through.
    /// Not async — only does atomic ops + scoped lock (m07-concurrency).
    pub fn allow_request(&self) -> bool {
        let mut state = self.state.lock();
        match state.cb_state {
            CbState::Closed => true,
            CbState::Open => {
                if let Some(last_fail) = state.last_failure_time
                    && last_fail.elapsed() > self.reset_timeout
                {
                    state.cb_state = CbState::HalfOpen;
                    return true;
                }
                false
            }
            CbState::HalfOpen => true,
        }
    }

    /// Record a successful request.
    /// Not async — only does scoped lock.
    pub fn record_success(&self) {
        let mut state = self.state.lock();
        if state.cb_state == CbState::HalfOpen {
            state.cb_state = CbState::Closed;
            state.failure_count = 0;
        }
    }

    /// Record a failed request.
    /// Not async — only does scoped lock.
    pub fn record_failure(&self) {
        let mut state = self.state.lock();
        state.failure_count += 1;
        if state.failure_count >= self.failure_threshold {
            state.cb_state = CbState::Open;
            state.last_failure_time = Some(Instant::now());
        }
    }

    /// Return current state for monitoring.
    pub fn state(&self) -> CbState {
        self.state.lock().cb_state
    }

    /// Return failure count for monitoring.
    pub fn failure_count(&self) -> u64 {
        self.state.lock().failure_count
    }
}
