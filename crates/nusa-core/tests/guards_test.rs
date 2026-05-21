//! Tests for resource guards: ResourceGuard, BackpressureGuard, validate_request_size, with_timeout.
//!
//! Covers: default values, backpressure permit acquisition/limits, request size validation, timeout behavior.

use nusa_core::{BackpressureGuard, EngineError, ResourceGuard, validate_request_size, with_timeout};
use std::time::Duration;

// ── ResourceGuard ──

#[test]
fn resource_guard_default_values() {
    let guard = ResourceGuard::default();
    assert_eq!(guard.max_request_bytes, 10 * 1024 * 1024);
    assert_eq!(guard.request_timeout_ms, 30_000);
    assert_eq!(guard.max_concurrent, 100);
}

#[test]
fn resource_guard_clone_preserves_values() {
    let guard = ResourceGuard::default();
    let cloned = guard.clone();
    assert_eq!(guard.max_request_bytes, cloned.max_request_bytes);
    assert_eq!(guard.request_timeout_ms, cloned.request_timeout_ms);
    assert_eq!(guard.max_concurrent, cloned.max_concurrent);
}

// ── BackpressureGuard ──

#[tokio::test]
async fn backpressure_guard_allows_concurrent_requests() {
    let guard = BackpressureGuard::new(5);
    let permit = guard.try_acquire().await;
    assert!(permit.is_some(), "should acquire permit when under limit");
}

#[tokio::test]
async fn backpressure_guard_blocks_when_at_capacity() {
    let guard = BackpressureGuard::new(1);

    let permit1 = guard.try_acquire().await;
    assert!(permit1.is_some());

    // With the permit held, try_acquire should return None
    let permit2 = guard.try_acquire().await;
    assert!(permit2.is_none(), "should not acquire permit when at capacity");

    // Drop the first permit, then try again
    drop(permit1);
    let permit3 = guard.try_acquire().await;
    assert!(permit3.is_some(), "should acquire permit after one is released");
}

#[tokio::test]
async fn backpressure_guard_multi_threaded_pressure() {
    let guard = BackpressureGuard::new(2);

    let p1 = guard.try_acquire().await;
    let p2 = guard.try_acquire().await;
    assert!(p1.is_some());
    assert!(p2.is_some());

    let p3 = guard.try_acquire().await;
    assert!(p3.is_none());

    drop(p1);
    let p4 = guard.try_acquire().await;
    assert!(p4.is_some());
}

// ── Request Size Validation ──

#[test]
fn validate_request_size_allows_under_limit() {
    assert!(validate_request_size(Some(100), 1024));
    assert!(validate_request_size(Some(1024), 1024));
}

#[test]
fn validate_request_size_rejects_over_limit() {
    assert!(!validate_request_size(Some(1025), 1024));
    assert!(!validate_request_size(Some(10_000_000), 1024));
}

#[test]
fn validate_request_size_allows_unknown_size() {
    // No Content-Length header — allow by default
    assert!(validate_request_size(None, 1024));
}

#[test]
fn validate_request_size_zero_allowed() {
    assert!(validate_request_size(Some(0), 1024));
}

// ── with_timeout ──

#[tokio::test]
async fn with_timeout_completes_within_limit() {
    let result = with_timeout(5_000, async {
        Ok::<_, EngineError>("success")
    }).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "success");
}

#[tokio::test]
async fn with_timeout_returns_error_on_timeout() {
    let result = with_timeout(10, async {
        tokio::time::sleep(Duration::from_secs(10)).await;
        Ok::<_, EngineError>("never")
    }).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), EngineError::Timeout));
}

#[tokio::test]
async fn with_timeout_propagates_other_errors() {
    let result = with_timeout(5_000, async {
        Err::<String, EngineError>(EngineError::ResourceLimit)
    }).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), EngineError::ResourceLimit));
}
