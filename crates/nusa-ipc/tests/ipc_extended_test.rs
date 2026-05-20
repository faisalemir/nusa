//! Extended IPC protocol tests: Keepalive, BroadcastEvent, TraceContext.
//!
//! rust-test-deep Phase 1: Core Exhaustive
//! rust-test-deep Phase 2: Security Exhaustive

use nusa_ipc::protocol::IpcMessage;
use nusa_ipc::trace::TraceContext;
use std::collections::HashMap;

// ── Keepalive Message ──

/// === Arrange ===
/// Keepalive message created via factory method.
/// === Act ===
/// Serialize and deserialize roundtrip.
/// === Assert ===
/// Variant preserved, timestamp present.
#[test]
fn ipc_keepalive_serializes_and_deserializes() {
    // === Arrange ===
    let msg = IpcMessage::keepalive();

    // === Act ===
    let framed = msg.to_framed_bytes().expect("keepalive must serialize");
    let decoded = IpcMessage::from_framed_bytes(&framed).expect("keepalive must deserialize");

    // === Assert ===
    assert!(
        matches!(decoded, IpcMessage::Keepalive { timestamp } if timestamp > 0),
        "keepalive must have valid timestamp"
    );
}

/// === Arrange ===
/// Multiple keepalive messages created.
/// === Act ===
/// Timestamps collected.
/// === Assert ===
/// Each timestamp is monotonically increasing.
#[test]
fn ipc_keepalive_timestamps_increase() {
    // === Arrange ===
    let m1 = IpcMessage::keepalive();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let m2 = IpcMessage::keepalive();

    // === Act ===
    let f1 = m1.to_framed_bytes().unwrap();
    let f2 = m2.to_framed_bytes().unwrap();
    let d1 = IpcMessage::from_framed_bytes(&f1).unwrap();
    let d2 = IpcMessage::from_framed_bytes(&f2).unwrap();

    // === Assert ===
    match (d1, d2) {
        (IpcMessage::Keepalive { timestamp: t1 }, IpcMessage::Keepalive { timestamp: t2 }) => {
            assert!(t2 >= t1, "timestamps must be monotonically increasing");
        }
        _ => panic!("expected Keepalive variants"),
    }
}

// ── BroadcastEvent Message ──

/// === Arrange ===
/// BroadcastEvent with channel, event name, data, tenants.
/// === Act ===
/// Serialize and deserialize roundtrip.
/// === Assert ===
/// All fields preserved.
#[test]
fn ipc_broadcast_event_roundtrip() {
    // === Arrange ===
    let msg = IpcMessage::broadcast_event(
        "private-channel.42".into(),
        "OrderCreated".into(),
        r#"{"id": 1}"#.into(),
        vec!["acme".into()],
    );

    // === Act ===
    let framed = msg.to_framed_bytes().expect("broadcast event must serialize");
    let decoded = IpcMessage::from_framed_bytes(&framed).expect("broadcast event must deserialize");

    // === Assert ===
    match decoded {
        IpcMessage::BroadcastEvent { channel, event, data, tenants } => {
            assert_eq!(channel, "private-channel.42");
            assert_eq!(event, "OrderCreated");
            assert_eq!(data, r#"{"id": 1}"#);
            assert_eq!(tenants, vec!["acme"]);
        }
        _ => panic!("expected BroadcastEvent variant"),
    }
}

/// === Arrange ===
/// BroadcastEvent with empty channel/event/data.
/// === Act ===
/// Roundtrip.
/// === Assert ===
/// Empty strings preserved.
#[test]
fn ipc_broadcast_event_empty_fields() {
    // === Arrange ===
    let msg = IpcMessage::BroadcastEvent {
        channel: String::new(),
        event: String::new(),
        data: String::new(),
        tenants: vec![],
    };

    // === Act ===
    let framed = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&framed).unwrap();

    // === Assert ===
    match decoded {
        IpcMessage::BroadcastEvent { channel, event, data, tenants } => {
            assert!(channel.is_empty());
            assert!(event.is_empty());
            assert!(data.is_empty());
            assert!(tenants.is_empty());
        }
        _ => panic!("expected BroadcastEvent"),
    }
}

/// === Arrange ===
/// BroadcastEvent with very large data payload.
/// === Act ===
/// Serialize 1MB data.
/// === Assert ===
/// Roundtrip succeeds.
#[test]
fn ipc_broadcast_event_large_payload() {
    // === Arrange ===
    let large_data = "x".repeat(1_000_000);
    let msg = IpcMessage::broadcast_event(
        "test".into(),
        "test".into(),
        large_data.clone(),
        vec![],
    );

    // === Act ===
    let framed = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&framed).unwrap();

    // === Assert ===
    match decoded {
        IpcMessage::BroadcastEvent { data, .. } => {
            assert_eq!(data, large_data);
        }
        _ => panic!("expected BroadcastEvent"),
    }
}

// ── TraceContext ──

/// === Arrange ===
/// TraceContext with valid traceparent.
/// === Act ===
/// Create from HTTP headers, inject into IPC headers.
/// === Assert ===
/// Trace context preserved through transformation.
#[test]
fn trace_context_from_http_headers() {
    // === Arrange ===
    let mut headers = http::HeaderMap::new();
    headers.insert("traceparent", "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".parse().unwrap());
    headers.insert("tracestate", "congo=t61rcWkgMzE".parse().unwrap());

    // === Act ===
    let ctx = TraceContext::from_http_headers(&headers);

    // === Assert ===
    assert_eq!(ctx.traceparent, "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01");
    assert_eq!(ctx.tracestate, Some("congo=t61rcWkgMzE".into()));
}

/// === Arrange ===
/// TraceContext from empty headers.
/// === Act ===
/// Create from headers with no trace info.
/// === Assert ===
/// Empty traceparent, no tracestate.
#[test]
fn trace_context_empty_headers() {
    // === Arrange ===
    let headers = http::HeaderMap::new();

    // === Act ===
    let ctx = TraceContext::from_http_headers(&headers);

    // === Assert ===
    assert!(ctx.traceparent.is_empty());
    assert!(ctx.tracestate.is_none());
}

/// === Arrange ===
/// TraceContext with valid values.
/// === Act ===
/// Inject into raw header map.
/// === Assert ===
/// Headers populated correctly.
#[test]
fn trace_context_inject_into_raw_headers() {
    // === Arrange ===
    let ctx = TraceContext {
        traceparent: "00-abc123-def456-01".into(),
        tracestate: Some("vendor=value".into()),
    };
    let mut headers = HashMap::new();

    // === Act ===
    ctx.inject_into(&mut headers);

    // === Assert ===
    assert_eq!(headers.get("traceparent"), Some(&vec!["00-abc123-def456-01".into()]));
    assert_eq!(headers.get("tracestate"), Some(&vec!["vendor=value".into()]));
}

/// === Arrange ===
/// IpcMessage::Request with trace_context.
/// === Act ===
/// Roundtrip serialization.
/// === Assert ===
/// Trace context preserved.
#[test]
fn ipc_request_with_trace_context_roundtrip() {
    // === Arrange ===
    let trace = TraceContext {
        traceparent: "00-abc-def-01".into(),
        tracestate: Some("test=1".into()),
    };
    let msg = IpcMessage::Request {
        id: nusa_ipc::RequestId::new(),
        method: "GET".into(),
        uri: "/api/test".into(),
        headers: Default::default(),
        query: Default::default(),
        post: Default::default(),
        cookies: Default::default(),
        files: vec![],
        body: None,
        server: Default::default(),
        timeout_ms: 5000,
        trace_context: Some(trace.clone()),
    };

    // === Act ===
    let framed = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&framed).unwrap();

    // === Assert ===
    match decoded {
        IpcMessage::Request { trace_context, .. } => {
            assert!(trace_context.is_some());
            let tc = trace_context.unwrap();
            assert_eq!(tc.traceparent, trace.traceparent);
            assert_eq!(tc.tracestate, trace.tracestate);
        }
        _ => panic!("expected Request variant"),
    }
}

// ── Security: Message Injection ──

/// === Arrange ===
/// IpcMessage with channel containing SQL injection patterns.
/// === Act ===
/// Roundtrip.
/// === Assert ===
/// Payload preserved exactly (not executed).
#[test]
fn ipc_broadcast_sql_injection_in_data() {
    // === Arrange ===
    let msg = IpcMessage::broadcast_event(
        "test".into(),
        "' OR 1=1 --".into(),
        "'; DROP TABLE users;--".into(),
        vec![],
    );

    // === Act ===
    let framed = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&framed).unwrap();

    // === Assert ===
    match decoded {
        IpcMessage::BroadcastEvent { event, data, .. } => {
            assert_eq!(event, "' OR 1=1 --");
            assert_eq!(data, "'; DROP TABLE users;--");
        }
        _ => panic!("expected BroadcastEvent"),
    }
}

/// === Arrange ===
/// IpcMessage with null bytes in data.
/// === Act ===
/// Roundtrip.
/// === Assert ===
/// Null bytes preserved.
#[test]
fn ipc_broadcast_null_bytes_preserved() {
    // === Arrange ===
    let msg = IpcMessage::broadcast_event(
        "test\0channel".into(),
        "event\0".into(),
        "data\0here".into(),
        vec![],
    );

    // === Act ===
    let framed = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&framed).unwrap();

    // === Assert ===
    match decoded {
        IpcMessage::BroadcastEvent { channel, event, data, .. } => {
            assert!(channel.contains('\0'));
            assert!(event.contains('\0'));
            assert!(data.contains('\0'));
        }
        _ => panic!("expected BroadcastEvent"),
    }
}
