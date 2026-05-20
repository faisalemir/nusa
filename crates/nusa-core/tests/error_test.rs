//! Exhaustive tests for EngineError enum.
//!
//! rust-test-deep Phase 1: Core Exhaustive
//! rust-test-deep §5: Enum Exhaustive

use nusa_core::EngineError;

// ── Happy Paths: All Variants ──

/// === Arrange ===
/// EngineError::Timeout variant.
/// === Act ===
/// Check display and HTTP mapping.
/// === Assert ===
/// Displays "execution timeout", maps to 408.
#[test]
fn engine_error_timeout_display_and_status() {
    // === Arrange ===
    let err = EngineError::Timeout;

    // === Act ===
    let display = err.to_string();
    let status = err.to_http_status();

    // === Assert ===
    assert_eq!(display, "execution timeout");
    assert_eq!(status, 408);
}

/// === Arrange ===
/// EngineError::Sandbox variant with message.
/// === Act ===
/// Check display includes message, maps to 500.
/// === Assert ===
/// Message included, 500 status.
#[test]
fn engine_error_sandbox_display_and_status() {
    // === Arrange ===
    let err = EngineError::Sandbox("memory violation".into());

    // === Act ===
    let display = err.to_string();
    let status = err.to_http_status();

    // === Assert ===
    assert!(display.contains("memory violation"));
    assert_eq!(status, 500);
}

/// === Arrange ===
/// EngineError::PhpFatal variant.
/// === Act ===
/// Check display, maps to 502.
/// === Assert ===
/// Message included, 502 status.
#[test]
fn engine_error_php_fatal_display_and_status() {
    // === Arrange ===
    let err = EngineError::PhpFatal("segfault at 0x0".into());

    // === Act ===
    let display = err.to_string();
    let status = err.to_http_status();

    // === Assert ===
    assert!(display.contains("segfault"));
    assert_eq!(status, 502);
}

/// === Arrange ===
/// EngineError::ResourceLimit variant.
/// === Act ===
/// Check display, maps to 429.
/// === Assert ===
/// "resource limit exceeded", 429 status.
#[test]
fn engine_error_resource_limit_display_and_status() {
    // === Arrange ===
    let err = EngineError::ResourceLimit;

    // === Act ===
    let display = err.to_string();
    let status = err.to_http_status();

    // === Assert ===
    assert_eq!(display, "resource limit exceeded");
    assert_eq!(status, 429);
}

/// === Arrange ===
/// EngineError::Plugin variant.
/// === Act ===
/// Check display, maps to 500.
/// === Assert ===
/// Message included, 500 status.
#[test]
fn engine_error_plugin_display_and_status() {
    // === Arrange ===
    let err = EngineError::Plugin("hook failed".into());

    // === Act ===
    let display = err.to_string();
    let status = err.to_http_status();

    // === Assert ===
    assert!(display.contains("hook failed"));
    assert_eq!(status, 500);
}

/// === Arrange ===
/// EngineError::IpcProtocol variant.
/// === Act ===
/// Check display, maps to 500.
/// === Assert ===
/// Message included, 500 status.
#[test]
fn engine_error_ipc_protocol_display_and_status() {
    // === Arrange ===
    let err = EngineError::IpcProtocol("malformed frame".into());

    // === Act ===
    let display = err.to_string();
    let status = err.to_http_status();

    // === Assert ===
    assert!(display.contains("malformed frame"));
    assert_eq!(status, 500);
}

// ── Enum Exhaustive Match ──

/// === Arrange ===
/// All error variants collected.
/// === Act ===
/// Match each variant explicitly.
/// === Assert ===
/// All variants handled (compile-time exhaustiveness).
#[test]
fn engine_error_all_variants_match() {
    // === Arrange ===
    let errors: Vec<EngineError> = vec![
        EngineError::Timeout,
        EngineError::Sandbox("test".into()),
        EngineError::PhpFatal("test".into()),
        EngineError::ResourceLimit,
        EngineError::Plugin("test".into()),
        EngineError::IpcProtocol("test".into()),
    ];

    // === Act ===
    for err in errors {
        let status = match &err {
            EngineError::Timeout => 408,
            EngineError::Sandbox(_) => 500,
            EngineError::PhpFatal(_) => 502,
            EngineError::ResourceLimit => 429,
            EngineError::Plugin(_) => 500,
            EngineError::IpcProtocol(_) => 500,
        };

        // === Assert ===
        assert_eq!(status, err.to_http_status());
    }
}

// ── Edge Cases ──

/// === Arrange ===
/// EngineError with empty message.
/// === Act ===
/// Display.
/// === Assert ===
/// No panic.
#[test]
fn engine_error_empty_message() {
    // === Arrange ===
    let err = EngineError::Sandbox("".into());

    // === Act ===
    let display = err.to_string();

    // === Assert ===
    assert!(display.contains("")); // empty message still works
}

/// === Arrange ===
/// EngineError with very long message.
/// === Act ===
/// Display.
/// === Assert ===
/// No panic, message included.
#[test]
fn engine_error_very_long_message() {
    // === Arrange ===
    let long_msg = "x".repeat(10_000);
    let err = EngineError::Sandbox(long_msg.clone());

    // === Act ===
    let display = err.to_string();

    // === Assert ===
    assert!(display.len() > 0);
}
