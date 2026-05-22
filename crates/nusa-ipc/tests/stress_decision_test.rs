//! Stress tests, decision logic tests, serialization tests, and error recovery tests
//! for nusa-ipc.
//!
//! Covers: IpcTransport, IpcMessage protocol, framing, serialization.

use nusa_ipc::protocol::{IpcMessage, RequestId};
use nusa_ipc::trace::TraceContext;
use std::collections::HashMap;

// ============================================================================
// Stress Tests: IPC Serialization Throughput
// ============================================================================

#[test]
fn ipc_serialization_throughput_1000_messages() {
    let msg = IpcMessage::Keepalive { timestamp: 0 };
    let start = std::time::Instant::now();

    for _ in 0..1000 {
        let bytes = msg.to_framed_bytes().unwrap();
        assert!(bytes.len() > 4);
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 5000,
        "1000 serializations took too long: {:?}",
        elapsed
    );
}

#[test]
fn ipc_serialization_throughput_10000_messages() {
    let msg = IpcMessage::Ping;
    let start = std::time::Instant::now();

    for _ in 0..10_000 {
        let bytes = msg.to_framed_bytes().unwrap();
        assert!(!bytes.is_empty());
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 10000,
        "10000 serializations took too long: {:?}",
        elapsed
    );
}

// ============================================================================
// Stress Tests: IPC Deserialization Throughput
// ============================================================================

#[test]
fn ipc_deserialization_throughput_1000_messages() {
    let msg = IpcMessage::Ping;
    let bytes = msg.to_framed_bytes().unwrap();
    let start = std::time::Instant::now();

    for _ in 0..1000 {
        let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();
        assert!(matches!(decoded, IpcMessage::Ping));
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 5000,
        "1000 deserializations took too long: {:?}",
        elapsed
    );
}

#[test]
fn ipc_deserialization_throughput_10000_messages() {
    let msg = IpcMessage::Ack;
    let bytes = msg.to_framed_bytes().unwrap();
    let start = std::time::Instant::now();

    for _ in 0..10_000 {
        let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();
        assert!(matches!(decoded, IpcMessage::Ack));
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 10000,
        "10000 deserializations took too long: {:?}",
        elapsed
    );
}

// ============================================================================
// Decision Logic Tests: IpcMessage Routing
// ============================================================================

#[test]
fn ipc_message_hello_variant_serializes_correctly() {
    let msg = IpcMessage::Hello {
        version: "1.0".into(),
        pid: 12345,
        capabilities: vec!["http".into(), "tasks".into()],
    };
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();
    match decoded {
        IpcMessage::Hello {
            version,
            pid,
            capabilities,
        } => {
            assert_eq!(version, "1.0");
            assert_eq!(pid, 12345);
            assert_eq!(capabilities, vec!["http", "tasks"]);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn ipc_message_ack_variant_serializes_correctly() {
    let msg = IpcMessage::Ack;
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();
    assert!(matches!(decoded, IpcMessage::Ack));
}

#[test]
fn ipc_message_request_variant_serializes_correctly() {
    let mut headers = HashMap::new();
    headers.insert("content-type".into(), vec!["application/json".into()]);
    let msg = IpcMessage::Request {
        id: RequestId::new(),
        method: "GET".into(),
        uri: "/index.php".into(),
        headers,
        query: HashMap::new(),
        post: HashMap::new(),
        cookies: HashMap::new(),
        files: vec![],
        body: Some(vec![1, 2, 3]),
        server: HashMap::new(),
        timeout_ms: 5000,
        trace_context: None,
    };
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();
    match decoded {
        IpcMessage::Request {
            method,
            uri,
            body,
            timeout_ms,
            ..
        } => {
            assert_eq!(method, "GET");
            assert_eq!(uri, "/index.php");
            assert_eq!(body, Some(vec![1, 2, 3]));
            assert_eq!(timeout_ms, 5000);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn ipc_message_response_variant_serializes_correctly() {
    let msg = IpcMessage::Response {
        id: RequestId::new(),
        status: 200,
        headers: HashMap::new(),
        body: vec![72, 101, 108, 108, 111],
        terminated: false,
    };
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();
    match decoded {
        IpcMessage::Response { status, body, .. } => {
            assert_eq!(status, 200);
            assert_eq!(body, vec![72, 101, 108, 108, 111]);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn ipc_message_keepalive_variant_serializes_correctly() {
    let msg = IpcMessage::Keepalive {
        timestamp: 1234567890,
    };
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();
    match decoded {
        IpcMessage::Keepalive { timestamp } => {
            assert_eq!(timestamp, 1234567890);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn ipc_message_ping_variant_serializes_correctly() {
    let msg = IpcMessage::Ping;
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();
    assert!(matches!(decoded, IpcMessage::Ping));
}

#[test]
fn ipc_message_pong_variant_serializes_correctly() {
    let msg = IpcMessage::Pong;
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();
    assert!(matches!(decoded, IpcMessage::Pong));
}

#[test]
fn ipc_message_shutdown_variant_serializes_correctly() {
    let msg = IpcMessage::Shutdown;
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();
    assert!(matches!(decoded, IpcMessage::Shutdown));
}

#[test]
fn ipc_message_recycle_variant_serializes_correctly() {
    let msg = IpcMessage::Recycle;
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();
    assert!(matches!(decoded, IpcMessage::Recycle));
}

#[test]
fn ipc_message_cancel_variant_serializes_correctly() {
    let msg = IpcMessage::Cancel {
        id: RequestId::new(),
    };
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();
    assert!(matches!(decoded, IpcMessage::Cancel { .. }));
}

#[test]
fn ipc_message_broadcast_event_variant_serializes_correctly() {
    let msg = IpcMessage::BroadcastEvent {
        channel: "orders".into(),
        event: "created".into(),
        data: r#"{"id":1}"#.into(),
        tenants: vec!["tenant-a".into()],
    };
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();
    match decoded {
        IpcMessage::BroadcastEvent {
            channel,
            event,
            data,
            tenants,
        } => {
            assert_eq!(channel, "orders");
            assert_eq!(event, "created");
            assert_eq!(data, r#"{"id":1}"#);
            assert_eq!(tenants, vec!["tenant-a"]);
        }
        _ => panic!("wrong variant"),
    }
}

// ============================================================================
// Decision Logic Tests: Protocol Version
// ============================================================================

#[test]
fn ipc_protocol_version_match_hello_accepted() {
    let msg = IpcMessage::Hello {
        version: "1.0".into(),
        pid: 1,
        capabilities: vec![],
    };
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes);
    assert!(decoded.is_ok(), "valid protocol version should be accepted");
}

#[test]
fn ipc_protocol_version_mismatch_parsed_anyway() {
    let msg = IpcMessage::Hello {
        version: "99.0".into(),
        pid: 1,
        capabilities: vec![],
    };
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes);
    assert!(
        decoded.is_ok(),
        "version mismatch should still parse (validation is semantic)"
    );
}

// ============================================================================
// Decision Logic Tests: Frame Size
// ============================================================================

#[test]
fn ipc_frame_under_max_accepted() {
    let msg = IpcMessage::Ping;
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes);
    assert!(decoded.is_ok());
}

#[test]
fn ipc_frame_large_body_accepted() {
    let msg = IpcMessage::Response {
        id: RequestId::new(),
        status: 200,
        headers: HashMap::new(),
        body: vec![0u8; 1_000_000],
        terminated: false,
    };
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes);
    assert!(decoded.is_ok());
}

#[test]
fn ipc_frame_empty_payload_rejected() {
    let data: &[u8] = &[];
    let decoded = IpcMessage::from_framed_bytes(data);
    assert!(decoded.is_err(), "empty payload should fail");
}

#[test]
fn ipc_frame_header_only_rejected() {
    let data: &[u8] = &[4, 0, 0, 0]; // 4 bytes header but no payload
    let decoded = IpcMessage::from_framed_bytes(data);
    assert!(decoded.is_err(), "header-only frame should fail");
}

#[test]
fn ipc_frame_partial_payload_rejected() {
    let partial_msg = IpcMessage::Ping;
    let full_bytes = partial_msg.to_framed_bytes().unwrap();
    // Take only half the bytes
    let partial = &full_bytes[..full_bytes.len() / 2];
    let decoded = IpcMessage::from_framed_bytes(partial);
    assert!(decoded.is_err(), "partial payload should fail");
}

// ============================================================================
// Decision Logic Tests: Heartbeat
// ============================================================================

#[test]
fn ipc_heartbeat_keepalive_message_valid() {
    let msg = IpcMessage::keepalive();
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();
    match decoded {
        IpcMessage::Keepalive { timestamp } => {
            assert!(timestamp > 0);
        }
        _ => panic!("expected Keepalive"),
    }
}

#[test]
fn ipc_heartbeat_ping_pong_roundtrip() {
    let ping = IpcMessage::Ping;
    let ping_bytes = ping.to_framed_bytes().unwrap();
    let decoded_ping = IpcMessage::from_framed_bytes(&ping_bytes).unwrap();
    assert!(matches!(decoded_ping, IpcMessage::Ping));

    let pong = IpcMessage::Pong;
    let pong_bytes = pong.to_framed_bytes().unwrap();
    let decoded_pong = IpcMessage::from_framed_bytes(&pong_bytes).unwrap();
    assert!(matches!(decoded_pong, IpcMessage::Pong));
}

// ============================================================================
// Serialization Tests: Roundtrip
// ============================================================================

#[test]
fn serialization_roundtrip_all_variants() {
    let messages: Vec<IpcMessage> = vec![
        IpcMessage::Hello {
            version: "1.0".into(),
            pid: 1,
            capabilities: vec!["http".into()],
        },
        IpcMessage::Ack,
        IpcMessage::Request {
            id: RequestId::new(),
            method: "POST".into(),
            uri: "/api".into(),
            headers: HashMap::new(),
            query: HashMap::new(),
            post: HashMap::new(),
            cookies: HashMap::new(),
            files: vec![],
            body: None,
            server: HashMap::new(),
            timeout_ms: 5000,
            trace_context: None,
        },
        IpcMessage::Response {
            id: RequestId::new(),
            status: 200,
            headers: HashMap::new(),
            body: vec![],
            terminated: false,
        },
        IpcMessage::Keepalive { timestamp: 0 },
        IpcMessage::Ping,
        IpcMessage::Pong,
        IpcMessage::Shutdown,
        IpcMessage::Recycle,
        IpcMessage::Cancel {
            id: RequestId::new(),
        },
        IpcMessage::BroadcastEvent {
            channel: "ch".into(),
            event: "ev".into(),
            data: "data".into(),
            tenants: vec![],
        },
    ];

    for msg in messages {
        let bytes = msg.to_framed_bytes().unwrap();
        let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();

        // Verify discriminant matches
        assert_eq!(
            std::mem::discriminant(&msg),
            std::mem::discriminant(&decoded),
            "variant mismatch after roundtrip: {:?}",
            msg
        );
    }
}

#[test]
fn serialization_roundtrip_request_with_body() {
    let msg = IpcMessage::Request {
        id: RequestId::new(),
        method: "POST".into(),
        uri: "/upload".into(),
        headers: HashMap::new(),
        query: HashMap::new(),
        post: HashMap::new(),
        cookies: HashMap::new(),
        files: vec![],
        body: Some(vec![1, 2, 3, 4, 5]),
        server: HashMap::new(),
        timeout_ms: 30000,
        trace_context: Some(TraceContext::default()),
    };
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();
    match decoded {
        IpcMessage::Request {
            body,
            trace_context,
            ..
        } => {
            assert_eq!(body, Some(vec![1, 2, 3, 4, 5]));
            assert!(trace_context.is_some());
        }
        _ => panic!("wrong variant"),
    }
}

// ============================================================================
// Serialization Tests: Malformed Input
// ============================================================================

#[test]
fn serialization_malformed_truncated_json_returns_error() {
    // Create a truncated frame: valid header but truncated JSON
    let mut bytes = (5u32).to_le_bytes().to_vec();
    bytes.extend_from_slice(b"{\"typ");
    let decoded = IpcMessage::from_framed_bytes(&bytes);
    assert!(decoded.is_err(), "truncated JSON should fail");
}

#[test]
fn serialization_malformed_extra_fields_ignored() {
    // Create a Ping message with extra fields via raw JSON
    let mut payload = serde_json::to_vec(&IpcMessage::Ping).unwrap();
    // Strip closing brace and add extra field
    payload.pop();
    payload.extend_from_slice(b",\"extra\":\"field\"}");
    let len = payload.len() as u32;
    let mut frame = len.to_le_bytes().to_vec();
    frame.extend_from_slice(&payload);

    let decoded = IpcMessage::from_framed_bytes(&frame);
    // serde_json ignores extra fields by default
    assert!(decoded.is_ok());
}

#[test]
fn serialization_malformed_wrong_type_returns_error() {
    // Create a frame with invalid JSON (wrong type for 'type' field)
    let payload = br#"{"type":123}"#;
    let len = payload.len() as u32;
    let mut frame = len.to_le_bytes().to_vec();
    frame.extend_from_slice(payload);

    let decoded = IpcMessage::from_framed_bytes(&frame);
    assert!(decoded.is_err(), "wrong type value should fail");
}

#[test]
fn serialization_malformed_missing_required_fields_returns_error() {
    // Request variant missing required fields
    let payload = br#"{"type":"Request"}"#;
    let len = payload.len() as u32;
    let mut frame = len.to_le_bytes().to_vec();
    frame.extend_from_slice(payload);

    let decoded = IpcMessage::from_framed_bytes(&frame);
    assert!(decoded.is_err(), "missing required fields should fail");
}

#[test]
fn serialization_malformed_unknown_variant_returns_error() {
    let payload = br#"{"type":"UnknownType"}"#;
    let len = payload.len() as u32;
    let mut frame = len.to_le_bytes().to_vec();
    frame.extend_from_slice(payload);

    let decoded = IpcMessage::from_framed_bytes(&frame);
    assert!(decoded.is_err(), "unknown variant should fail");
}

// ============================================================================
// Serialization Tests: Empty vs Null
// ============================================================================

#[test]
fn serialization_empty_body_vs_none_distinguished() {
    let msg_with_none = IpcMessage::Request {
        id: RequestId::new(),
        method: "GET".into(),
        uri: "/".into(),
        headers: HashMap::new(),
        query: HashMap::new(),
        post: HashMap::new(),
        cookies: HashMap::new(),
        files: vec![],
        body: None,
        server: HashMap::new(),
        timeout_ms: 5000,
        trace_context: None,
    };

    let msg_with_empty_vec = IpcMessage::Request {
        id: RequestId::new(),
        method: "GET".into(),
        uri: "/".into(),
        headers: HashMap::new(),
        query: HashMap::new(),
        post: HashMap::new(),
        cookies: HashMap::new(),
        files: vec![],
        body: Some(vec![]),
        server: HashMap::new(),
        timeout_ms: 5000,
        trace_context: None,
    };

    let bytes_none = msg_with_none.to_framed_bytes().unwrap();
    let bytes_empty = msg_with_empty_vec.to_framed_bytes().unwrap();

    // Different serialization
    assert_ne!(bytes_none, bytes_empty);

    // Roundtrip preserves the distinction
    let decoded_none = IpcMessage::from_framed_bytes(&bytes_none).unwrap();
    let decoded_empty = IpcMessage::from_framed_bytes(&bytes_empty).unwrap();

    match decoded_none {
        IpcMessage::Request { body, .. } => assert!(body.is_none()),
        _ => panic!("wrong variant"),
    }
    match decoded_empty {
        IpcMessage::Request { body, .. } => assert!(body.as_ref().unwrap().is_empty()),
        _ => panic!("wrong variant"),
    }
}

#[test]
fn serialization_null_json_body_parsed_as_none() {
    // Manual JSON with body: null
    let json = r#"{"type":"Request","id":"00000000-0000-0000-0000-000000000000","method":"GET","uri":"/","headers":{},"query":{},"post":{},"cookies":{},"files":[],"body":null,"server":{},"timeout_ms":5000}"#;
    let len = json.len() as u32;
    let mut frame = len.to_le_bytes().to_vec();
    frame.extend_from_slice(json.as_bytes());

    let decoded = IpcMessage::from_framed_bytes(&frame).unwrap();
    match decoded {
        IpcMessage::Request { body, .. } => {
            assert!(body.is_none(), "null body should parse as None");
        }
        _ => panic!("wrong variant"),
    }
}

// ============================================================================
// Serialization Tests: Nesting Depth
// ============================================================================

#[test]
fn serialization_deeply_nested_payload_no_stack_overflow() {
    // Create a deeply nested JSON value and embed it in a Request body
    let mut nested = serde_json::json!("leaf");
    for _ in 0..50 {
        nested = serde_json::json!({ "inner": nested });
    }

    let body = serde_json::to_vec(&nested).unwrap_or_default();
    let msg = IpcMessage::Request {
        id: RequestId::new(),
        method: "POST".into(),
        uri: "/deep".into(),
        headers: HashMap::new(),
        query: HashMap::new(),
        post: HashMap::new(),
        cookies: HashMap::new(),
        files: vec![],
        body: Some(body),
        server: HashMap::new(),
        timeout_ms: 5000,
        trace_context: None,
    };

    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes);
    assert!(
        decoded.is_ok(),
        "deep nesting should not cause stack overflow"
    );
}

// ============================================================================
// Decision Logic Tests: RequestId
// ============================================================================

#[test]
fn request_id_new_generates_unique() {
    let id1 = RequestId::new();
    let id2 = RequestId::new();
    assert_ne!(id1, id2, "RequestIds must be unique");
}

#[test]
fn request_id_default_equals_new() {
    let id1 = RequestId::default();
    let _id2 = RequestId::new();
    // Both are UUIDs, so they should not be equal (statistically)
    // But we just verify both are valid
    let id3 = RequestId::default();
    assert_ne!(id1, id3, "defaults should also be unique");
}

#[test]
fn request_id_copy_eq_self() {
    let id = RequestId::new();
    let copy = id;
    assert_eq!(id, copy);
}

// ============================================================================
// Decision Logic Tests: TraceContext
// ============================================================================

#[test]
fn trace_context_default_is_valid() {
    let ctx = TraceContext::default();
    assert!(ctx.traceparent.is_empty());
    assert!(ctx.tracestate.is_none());
}

#[test]
fn trace_context_from_raw_headers_empty() {
    let headers: HashMap<String, Vec<String>> = HashMap::new();
    let ctx = TraceContext::from_raw_headers(&headers);
    assert!(ctx.traceparent.is_empty());
}

#[test]
fn trace_context_from_raw_headers_with_traceparent() {
    let mut headers = HashMap::new();
    headers.insert(
        "traceparent".into(),
        vec!["00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".into()],
    );
    let ctx = TraceContext::from_raw_headers(&headers);
    assert!(
        !ctx.traceparent.is_empty(),
        "traceparent should populate traceparent field"
    );
}

#[test]
fn trace_context_from_raw_headers_with_tracestate() {
    let mut headers = HashMap::new();
    headers.insert("traceparent".into(), vec!["00-abc123".into()]);
    headers.insert("tracestate".into(), vec!["vendor=key".into()]);
    let ctx = TraceContext::from_raw_headers(&headers);
    assert_eq!(ctx.tracestate, Some("vendor=key".into()));
}

// ============================================================================
// Error Recovery Tests
// ============================================================================

#[test]
fn ipc_error_truncated_frame_returns_framing_error() {
    let data: &[u8] = &[1, 0, 0, 0]; // header says 1 byte, but no payload
    let result = IpcMessage::from_framed_bytes(data);
    assert!(result.is_err());
}

#[test]
fn ipc_error_zero_length_frame_returns_error() {
    let data: &[u8] = &[];
    let result = IpcMessage::from_framed_bytes(data);
    assert!(result.is_err());
}

#[test]
fn ipc_error_corrupted_header_returns_error() {
    // Payload larger than declared in header
    let msg = IpcMessage::Ping;
    let valid_bytes = msg.to_framed_bytes().unwrap();
    // Corrupt the length header to be much larger
    let mut corrupted = valid_bytes.clone();
    corrupted[0] = 0xFF;
    corrupted[1] = 0xFF;
    corrupted[2] = 0xFF;
    corrupted[3] = 0xFF;

    let result = IpcMessage::from_framed_bytes(&corrupted);
    assert!(result.is_err(), "corrupted header should fail");
}
