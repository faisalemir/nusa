//! IPC integration tests.
//!
//! Tests full IPC request-response cycle, trace context, heartbeat, error recovery, and flow control.

use std::collections::HashMap;
use std::time::Duration;

use nusa_ipc::protocol::{IpcMessage, RequestId};
use nusa_ipc::trace::TraceContext;

// ── Full IPC Request-Response ──

#[tokio::test]
async fn ipc_integration_full_request_response() {
    let request = IpcMessage::Request {
        id: RequestId::new(),
        method: "GET".into(),
        uri: "/index.php".into(),
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

    let bytes = request.to_framed_bytes().expect("serialize request");
    assert!(
        bytes.len() > 4,
        "framed bytes must include 4-byte length prefix"
    );

    let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize request");
    match parsed {
        IpcMessage::Request { method, uri, .. } => {
            assert_eq!(method, "GET");
            assert_eq!(uri, "/index.php");
        }
        _ => panic!("expected Request message"),
    }
}

// ── IPC with Trace Context ──

#[tokio::test]
async fn ipc_integration_trace_context_passed() {
    let mut headers = HashMap::new();
    headers.insert(
        "traceparent".into(),
        vec!["00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".into()],
    );

    let trace_context = TraceContext::from_raw_headers(&headers);
    assert!(!trace_context.traceparent.is_empty());

    let request = IpcMessage::Request {
        id: RequestId::new(),
        method: "GET".into(),
        uri: "/index.php".into(),
        headers: headers.clone(),
        query: HashMap::new(),
        post: HashMap::new(),
        cookies: HashMap::new(),
        files: vec![],
        body: None,
        server: HashMap::new(),
        timeout_ms: 5000,
        trace_context: Some(trace_context.clone()),
    };

    let bytes = request.to_framed_bytes().expect("serialize with trace");
    let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize");

    match parsed {
        IpcMessage::Request { trace_context, .. } => {
            assert!(trace_context.is_some());
        }
        _ => panic!("expected Request"),
    }
}

// ── IPC Heartbeat ──

#[tokio::test]
async fn ipc_integration_heartbeat_successful_cycle() {
    let heartbeat = IpcMessage::keepalive();
    let bytes = heartbeat.to_framed_bytes().expect("serialize heartbeat");
    let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize heartbeat");

    match parsed {
        IpcMessage::Keepalive { timestamp } => {
            assert!(timestamp > 0);
        }
        _ => panic!("expected Keepalive"),
    }
}

// ── IPC Version Handshake ──

#[tokio::test]
async fn ipc_integration_hello_exchanged_version_matched() {
    let hello = IpcMessage::Hello {
        version: "1.0".into(),
        pid: std::process::id(),
        capabilities: vec!["http".into(), "tasks".into()],
    };

    let bytes = hello.to_framed_bytes().expect("serialize hello");
    let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize hello");

    match parsed {
        IpcMessage::Hello {
            version,
            pid,
            capabilities,
        } => {
            assert_eq!(version, "1.0");
            assert_eq!(pid, std::process::id());
            assert!(capabilities.contains(&"http".to_string()));
            assert!(capabilities.contains(&"tasks".to_string()));
        }
        _ => panic!("expected Hello"),
    }
}

#[tokio::test]
async fn ipc_integration_ack_received() {
    let ack = IpcMessage::Ack;
    let bytes = ack.to_framed_bytes().expect("serialize ack");
    let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize ack");

    assert!(matches!(parsed, IpcMessage::Ack));
}

// ── IPC Error Recovery ──

#[tokio::test]
async fn ipc_integration_malformed_message_rejected() {
    let malformed = vec![0xFF, 0xFF, 0xFF, 0xFF, 0x00]; // invalid length prefix
    let result = IpcMessage::from_framed_bytes(&malformed);
    assert!(result.is_err());
}

#[tokio::test]
async fn ipc_integration_truncated_frame_rejected() {
    let truncated = vec![0x04, 0x00, 0x00, 0x00]; // says 4 bytes but no payload
    let result = IpcMessage::from_framed_bytes(&truncated);
    assert!(result.is_err());
}

#[tokio::test]
async fn ipc_integration_empty_frame_rejected() {
    let empty: Vec<u8> = vec![];
    let result = IpcMessage::from_framed_bytes(&empty);
    assert!(result.is_err());
}

// ── IPC FIFO Ordering ──

#[tokio::test]
async fn ipc_integration_fifo_ordering_preserved() {
    let messages: Vec<IpcMessage> = (0..10)
        .map(|i| IpcMessage::Response {
            id: RequestId::new(),
            status: 200,
            headers: HashMap::new(),
            body: format!("response-{}", i).into_bytes(),
            terminated: false,
        })
        .collect();

    // Serialize and deserialize each, verify order
    for (i, msg) in messages.iter().enumerate() {
        let bytes = msg.to_framed_bytes().expect("serialize");
        let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize");

        match parsed {
            IpcMessage::Response { body, .. } => {
                assert_eq!(body, format!("response-{}", i).into_bytes());
            }
            _ => panic!("expected Response"),
        }
    }
}

// ── IPC Flow Control ──

#[test]
fn ipc_integration_request_id_unique() {
    let id1 = RequestId::new();
    let id2 = RequestId::new();
    assert_ne!(id1, id2, "RequestIds must be unique");
}

// ── IPC Large Message ──

#[tokio::test]
async fn ipc_integration_10mb_message_sent_received() {
    let large_body = vec![0x42u8; 10 * 1024 * 1024]; // 10MB

    let response = IpcMessage::Response {
        id: RequestId::new(),
        status: 200,
        headers: HashMap::new(),
        body: large_body.clone(),
        terminated: false,
    };

    let bytes = response
        .to_framed_bytes()
        .expect("serialize large response");
    assert!(bytes.len() > 10 * 1024 * 1024);

    let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize large response");
    match parsed {
        IpcMessage::Response { body, .. } => {
            assert_eq!(body.len(), 10 * 1024 * 1024);
            assert_eq!(body, large_body);
        }
        _ => panic!("expected Response"),
    }
}

// ── IPC Many Small Messages ──

#[tokio::test]
async fn ipc_integration_10000_tiny_messages_no_overhead() {
    let start = std::time::Instant::now();

    for i in 0..10000 {
        let msg = IpcMessage::Response {
            id: RequestId::new(),
            status: 200,
            headers: HashMap::new(),
            body: vec![i as u8],
            terminated: false,
        };

        let bytes = msg.to_framed_bytes().expect("serialize tiny");
        let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize tiny");

        match parsed {
            IpcMessage::Response { body, .. } => {
                assert_eq!(body, vec![i as u8]);
            }
            _ => panic!("expected Response"),
        }
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "10000 tiny messages should serialize/deserialize quickly, took {:?}",
        elapsed
    );
}

// ── IPC Broadcast Event ──

#[test]
fn ipc_integration_broadcast_event_serialization() {
    let event = IpcMessage::broadcast_event(
        "channel-1".into(),
        "update".into(),
        r#"{"key":"value"}"#.into(),
        vec!["tenant-a".into(), "tenant-b".into()],
    );

    let bytes = event.to_framed_bytes().expect("serialize broadcast");
    let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize broadcast");

    match parsed {
        IpcMessage::BroadcastEvent {
            channel,
            event,
            data,
            tenants,
        } => {
            assert_eq!(channel, "channel-1");
            assert_eq!(event, "update");
            assert_eq!(data, r#"{"key":"value"}"#);
            assert_eq!(tenants.len(), 2);
        }
        _ => panic!("expected BroadcastEvent"),
    }
}

// ── IPC Shutdown/Recycle ──

#[tokio::test]
async fn ipc_integration_control_signals() {
    for msg in &[
        IpcMessage::Shutdown,
        IpcMessage::Recycle,
        IpcMessage::Ping,
        IpcMessage::Pong,
    ] {
        let bytes = msg.to_framed_bytes().expect("serialize control");
        let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize control");
        assert!(
            matches!(
                &parsed,
                IpcMessage::Shutdown | IpcMessage::Recycle | IpcMessage::Ping | IpcMessage::Pong
            ),
            "expected control message"
        );
    }
}
