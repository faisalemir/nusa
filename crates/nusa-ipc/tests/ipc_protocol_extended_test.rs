//! Extended tests for nusa-ipc protocol messages.
//!
//! Covers: exhaustive IpcMessage variants, FileUpload roundtrip, edge cases, security.

use std::collections::HashMap;

use nusa_ipc::{IpcMessage, RequestId, TraceContext};

// ── IpcMessage Exhaustive Variants ──

#[test]
fn ipc_message_hello_empty_capabilities() {
    let msg = IpcMessage::Hello {
        version: "1.0".to_string(),
        pid: 12345,
        capabilities: vec![],
    };
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();

    match decoded {
        IpcMessage::Hello { version, pid, capabilities } => {
            assert_eq!(version, "1.0");
            assert_eq!(pid, 12345);
            assert!(capabilities.is_empty());
        }
        _ => panic!("expected Hello"),
    }
}

#[test]
fn ipc_message_hello_many_capabilities() {
    let caps: Vec<String> = (0..100).map(|i| format!("cap-{}", i)).collect();
    let msg = IpcMessage::Hello {
        version: "1.0".to_string(),
        pid: 1,
        capabilities: caps.clone(),
    };
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();

    match decoded {
        IpcMessage::Hello { capabilities, .. } => {
            assert_eq!(capabilities.len(), 100);
            assert_eq!(capabilities, caps);
        }
        _ => panic!("expected Hello"),
    }
}

#[test]
fn ipc_message_hello_very_long_version() {
    let version = "v".repeat(10_000);
    let msg = IpcMessage::Hello {
        version: version.clone(),
        pid: 1,
        capabilities: vec!["http".to_string()],
    };
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();

    match decoded {
        IpcMessage::Hello { version: v, .. } => {
            assert_eq!(v, version);
        }
        _ => panic!("expected Hello"),
    }
}

#[test]
fn ipc_message_ack_roundtrip() {
    let msg = IpcMessage::Ack;
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();
    assert!(matches!(decoded, IpcMessage::Ack));
}

#[test]
fn ipc_message_request_empty_fields() {
    let msg = IpcMessage::Request {
        id: RequestId::new(),
        method: String::new(),
        uri: String::new(),
        headers: HashMap::new(),
        query: HashMap::new(),
        post: HashMap::new(),
        cookies: HashMap::new(),
        files: vec![],
        body: None,
        server: HashMap::new(),
        timeout_ms: 0,
        trace_context: None,
    };
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();

    match decoded {
        IpcMessage::Request { method, uri, timeout_ms, body, .. } => {
            assert!(method.is_empty());
            assert!(uri.is_empty());
            assert_eq!(timeout_ms, 0);
            assert!(body.is_none());
        }
        _ => panic!("expected Request"),
    }
}

#[test]
fn ipc_message_request_status_zero() {
    let msg = IpcMessage::Response {
        id: RequestId::new(),
        status: 0,
        headers: HashMap::new(),
        body: vec![],
        terminated: false,
    };
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();

    match decoded {
        IpcMessage::Response { status, .. } => {
            assert_eq!(status, 0);
        }
        _ => panic!("expected Response"),
    }
}

#[test]
fn ipc_message_response_status_max() {
    let msg = IpcMessage::Response {
        id: RequestId::new(),
        status: 999,
        headers: HashMap::new(),
        body: vec![0u8; 1000],
        terminated: true,
    };
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();

    match decoded {
        IpcMessage::Response { status, terminated, body, .. } => {
            assert_eq!(status, 999);
            assert!(terminated);
            assert_eq!(body.len(), 1000);
        }
        _ => panic!("expected Response"),
    }
}

#[test]
fn ipc_message_cancel_roundtrip() {
    let id = RequestId::new();
    let msg = IpcMessage::Cancel { id };
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();

    match decoded {
        IpcMessage::Cancel { id: decoded_id } => {
            assert_eq!(decoded_id, id);
        }
        _ => panic!("expected Cancel"),
    }
}

#[test]
fn ipc_message_recycle_roundtrip() {
    let msg = IpcMessage::Recycle;
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();
    assert!(matches!(decoded, IpcMessage::Recycle));
}

#[test]
fn ipc_message_ping_pong_roundtrip() {
    let ping = IpcMessage::Ping;
    let ping_bytes = ping.to_framed_bytes().unwrap();
    let ping_decoded = IpcMessage::from_framed_bytes(&ping_bytes).unwrap();
    assert!(matches!(ping_decoded, IpcMessage::Ping));

    let pong = IpcMessage::Pong;
    let pong_bytes = pong.to_framed_bytes().unwrap();
    let pong_decoded = IpcMessage::from_framed_bytes(&pong_bytes).unwrap();
    assert!(matches!(pong_decoded, IpcMessage::Pong));
}

#[test]
fn ipc_message_shutdown_roundtrip() {
    let msg = IpcMessage::Shutdown;
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();
    assert!(matches!(decoded, IpcMessage::Shutdown));
}

#[test]
fn ipc_message_keepalive_timestamp_monotonic() {
    let msg1 = IpcMessage::keepalive();
    std::thread::sleep(std::time::Duration::from_millis(10));
    let msg2 = IpcMessage::keepalive();

    match (msg1, msg2) {
        (IpcMessage::Keepalive { timestamp: t1 }, IpcMessage::Keepalive { timestamp: t2 }) => {
            assert!(t2 >= t1, "timestamps must be monotonic");
        }
        _ => panic!("expected Keepalive"),
    }
}

// ── FileUpload Edge Cases ──

#[test]
fn ipc_message_file_upload_special_chars_in_filename() {
    let msg = IpcMessage::Request {
        id: RequestId::new(),
        method: "POST".to_string(),
        uri: "/upload".to_string(),
        headers: HashMap::new(),
        query: HashMap::new(),
        post: HashMap::new(),
        cookies: HashMap::new(),
        files: vec![nusa_ipc::FileUpload {
            name: "file".to_string(),
            filename: "my file (1).txt".to_string(),
            mime_type: "text/plain".to_string(),
            size: 100,
            tmp_path: "/tmp/upload_äöü.txt".to_string(),
        }],
        body: None,
        server: HashMap::new(),
        timeout_ms: 5000,
        trace_context: None,
    };
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();

    match decoded {
        IpcMessage::Request { files, .. } => {
            assert_eq!(files.len(), 1);
            assert_eq!(files[0].filename, "my file (1).txt");
            assert!(files[0].tmp_path.contains("äöü"));
        }
        _ => panic!("expected Request"),
    }
}

#[test]
fn ipc_message_file_upload_null_byte_in_filename() {
    let msg = IpcMessage::Request {
        id: RequestId::new(),
        method: "POST".to_string(),
        uri: "/upload".to_string(),
        headers: HashMap::new(),
        query: HashMap::new(),
        post: HashMap::new(),
        cookies: HashMap::new(),
        files: vec![nusa_ipc::FileUpload {
            name: "file".to_string(),
            filename: "evil.txt\0.php".to_string(),
            mime_type: "application/octet-stream".to_string(),
            size: 0,
            tmp_path: "/tmp".to_string(),
        }],
        body: None,
        server: HashMap::new(),
        timeout_ms: 5000,
        trace_context: None,
    };
    // Null bytes in JSON strings should roundtrip correctly
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();

    match decoded {
        IpcMessage::Request { files, .. } => {
            assert_eq!(files[0].filename, "evil.txt\0.php");
        }
        _ => panic!("expected Request"),
    }
}

// ── BroadcastEvent Edge Cases ──

#[test]
fn ipc_message_broadcast_event_empty_fields() {
    let msg = IpcMessage::BroadcastEvent {
        channel: String::new(),
        event: String::new(),
        data: String::new(),
        tenants: vec![],
    };
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();

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

#[test]
fn ipc_message_broadcast_many_tenants() {
    let tenants: Vec<String> = (0..1000).map(|i| format!("tenant-{}", i)).collect();
    let msg = IpcMessage::BroadcastEvent {
        channel: "global".to_string(),
        event: "update".to_string(),
        data: "{}".to_string(),
        tenants: tenants.clone(),
    };
    let bytes = msg.to_framed_bytes().unwrap();
    let decoded = IpcMessage::from_framed_bytes(&bytes).unwrap();

    match decoded {
        IpcMessage::BroadcastEvent { tenants: decoded_tenants, .. } => {
            assert_eq!(decoded_tenants.len(), 1000);
        }
        _ => panic!("expected BroadcastEvent"),
    }
}

// ── TraceContext ──

#[test]
fn trace_context_from_http_headers_with_traceparent() {
    let mut headers = HashMap::new();
    headers.insert(
        "traceparent".to_string(),
        vec!["00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_string()],
    );
    let ctx = TraceContext::from_raw_headers(&headers);
    assert!(!ctx.traceparent.is_empty());
    assert!(ctx.traceparent.contains("4bf92f3577b34da6a3ce929d0e0e4736"));
}

#[test]
fn trace_context_from_http_headers_empty() {
    let headers = HashMap::new();
    let ctx = TraceContext::from_raw_headers(&headers);
    assert!(ctx.traceparent.is_empty());
    assert!(ctx.tracestate.is_none());
}

#[test]
fn trace_context_from_http_headers_malformed() {
    let mut headers = HashMap::new();
    headers.insert("traceparent".to_string(), vec!["invalid".to_string()]);
    let ctx = TraceContext::from_raw_headers(&headers);
    // Still gets the string, just doesn't parse it
    assert_eq!(ctx.traceparent, "invalid");
}

#[test]
fn trace_context_with_tracestate() {
    let mut headers = HashMap::new();
    headers.insert(
        "traceparent".to_string(),
        vec!["00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_string()],
    );
    headers.insert(
        "tracestate".to_string(),
        vec!["vendor=value".to_string()],
    );
    let ctx = TraceContext::from_raw_headers(&headers);
    assert!(ctx.tracestate.is_some());
    assert_eq!(ctx.tracestate.unwrap(), "vendor=value");
}

#[test]
fn trace_context_default_is_empty() {
    let ctx = TraceContext::default();
    assert!(ctx.traceparent.is_empty());
    assert!(ctx.tracestate.is_none());
}
