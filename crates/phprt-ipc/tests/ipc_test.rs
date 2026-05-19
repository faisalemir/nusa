//! Integration tests for phprt-ipc crate
//!
//! Skills applied:
//! - `m06-error-handling`: Result propagation, error context in assertions
//! - `coding-guidelines`: assert! with descriptive messages, expect() with reason
//! - `m15-anti-pattern`: No unwrap() in production paths, thorough error case testing

use bytes::BytesMut;
use phprt_ipc::framing::IpcCodec;
use phprt_ipc::protocol::{IpcMessage, RequestId};
use tokio_util::codec::{Decoder, Encoder};

// ── RequestId ──

#[test]
fn request_id_generates_unique() {
    let a = RequestId::new();
    let b = RequestId::new();
    assert_ne!(a, b, "RequestId must generate unique values");
}

#[test]
fn request_id_default_also_unique() {
    let a = RequestId::default();
    let b = RequestId::new();
    assert_ne!(a, b, "Default must also produce a unique ID");
}

// ── IPC Message Serialization ──

#[test]
fn serialize_hello_roundtrip() {
    let msg = IpcMessage::Hello {
        version: "1.0".to_string(),
        pid: 12345,
        capabilities: vec!["http".to_string(), "tasks".to_string()],
    };

    let framed = msg.to_framed_bytes().expect("Hello must serialize");
    let decoded = IpcMessage::from_framed_bytes(&framed).expect("Hello must deserialize");

    match decoded {
        IpcMessage::Hello { version, pid, capabilities } => {
            assert_eq!(version, "1.0");
            assert_eq!(pid, 12345);
            assert_eq!(capabilities, vec!["http", "tasks"]);
        }
        _ => panic!("expected Hello variant"),
    }
}

#[test]
fn serialize_request_roundtrip() {
    let id = RequestId::new();
    let msg = IpcMessage::Request {
        id,
        method: "GET".to_string(),
        uri: "/api/users".to_string(),
        headers: Default::default(),
        query: Default::default(),
        post: Default::default(),
        cookies: Default::default(),
        files: vec![],
        body: None,
        server: Default::default(),
        timeout_ms: 30_000,
    };

    let framed = msg.to_framed_bytes().expect("Request must serialize");
    let decoded = IpcMessage::from_framed_bytes(&framed).expect("Request must deserialize");

    match decoded {
        IpcMessage::Request { method, uri, timeout_ms, .. } => {
            assert_eq!(method, "GET");
            assert_eq!(uri, "/api/users");
            assert_eq!(timeout_ms, 30_000);
        }
        _ => panic!("expected Request variant"),
    }
}

#[test]
fn serialize_response_roundtrip() {
    let id = RequestId::new();
    let msg = IpcMessage::Response {
        id,
        status: 200,
        headers: Default::default(),
        body: b"hello world".to_vec(),
        terminated: true,
    };

    let framed = msg.to_framed_bytes().expect("Response must serialize");
    let decoded = IpcMessage::from_framed_bytes(&framed).expect("Response must deserialize");

    match decoded {
        IpcMessage::Response { status, body, terminated, .. } => {
            assert_eq!(status, 200);
            assert_eq!(body, b"hello world");
            assert!(terminated);
        }
        _ => panic!("expected Response variant"),
    }
}

#[test]
fn serialize_control_signals() {
    for msg in [
        IpcMessage::Ping,
        IpcMessage::Pong,
        IpcMessage::Shutdown,
        IpcMessage::Recycle,
    ] {
        let framed = msg.to_framed_bytes().expect("control signal must serialize");
        let decoded = IpcMessage::from_framed_bytes(&framed).expect("control signal must deserialize");

        // Type-level check: variant matches original
        assert_eq!(
            std::mem::discriminant(&msg),
            std::mem::discriminant(&decoded),
            "control signal roundtrip must preserve variant",
        );
    }
}

#[test]
fn serialize_cancel_with_id() {
    let id = RequestId::new();
    let msg = IpcMessage::Cancel { id };

    let framed = msg.to_framed_bytes().expect("Cancel must serialize");
    let decoded = IpcMessage::from_framed_bytes(&framed).expect("Cancel must deserialize");

    match decoded {
        IpcMessage::Cancel { id: dec_id } => assert_eq!(id, dec_id),
        _ => panic!("expected Cancel variant"),
    }
}

// ── IPC Framing Codec (tokio-util) ──

#[test]
fn codec_encode_decode_single_frame() {
    let msg = IpcMessage::Shutdown;
    let mut codec = IpcCodec::new();
    let mut buf = BytesMut::new();

    codec.encode(msg, &mut buf).expect("encode must succeed");
    let decoded = codec.decode(&mut buf).expect("decode must succeed").expect("must produce a message");

    assert!(matches!(decoded, IpcMessage::Shutdown));
    assert!(buf.is_empty(), "buffer must be fully consumed");
}

#[test]
fn codec_handles_multiple_frames_in_buffer() {
    let mut codec = IpcCodec::new();
    let mut buf = BytesMut::new();

    // Encode two messages
    codec.encode(IpcMessage::Ping, &mut buf).expect("encode Ping");
    codec.encode(IpcMessage::Pong, &mut buf).expect("encode Pong");

    let first = codec.decode(&mut buf).expect("decode first");
    let second = codec.decode(&mut buf).expect("decode second");

    assert!(matches!(first, Some(IpcMessage::Ping)));
    assert!(matches!(second, Some(IpcMessage::Pong)));
}

#[test]
fn codec_returns_none_for_incomplete_header() {
    let mut codec = IpcCodec::new();
    let mut buf = BytesMut::from(&[0u8; 3][..]); // less than 4 bytes

    let result = codec.decode(&mut buf).expect("decode should not error, just return None");
    assert!(result.is_none(), "incomplete header must return None");
}

#[test]
fn codec_returns_none_for_incomplete_payload() {
    let mut codec = IpcCodec::new();
    let mut buf = BytesMut::new();

    // Encode a large message, then truncate the buffer
    codec
        .encode(
            IpcMessage::Request {
                id: RequestId::new(),
                method: "POST".into(),
                uri: "/".into(),
                headers: Default::default(),
                query: Default::default(),
                post: Default::default(),
                cookies: Default::default(),
                files: vec![],
                body: Some(vec![0u8; 1000]),
                server: Default::default(),
                timeout_ms: 1000,
            },
            &mut buf,
        )
        .expect("encode must succeed");

    // Remove last 10 bytes to simulate incomplete payload
    buf.truncate(buf.len() - 10);

    let result = codec.decode(&mut buf).expect("decode should not error on incomplete data");
    assert!(result.is_none(), "incomplete payload must return None");
}

#[test]
fn codec_respects_max_frame_size() {
    let mut codec = IpcCodec::with_max_frame_size(50);

    let msg = IpcMessage::Request {
        id: RequestId::new(),
        method: "GET".into(),
        uri: "/".into(),
        headers: Default::default(),
        query: Default::default(),
        post: Default::default(),
        cookies: Default::default(),
        files: vec![],
        body: Some(vec![0u8; 100]), // exceeds max
        server: Default::default(),
        timeout_ms: 1000,
    };

    let mut buf = BytesMut::new();
    codec.encode(msg, &mut buf).expect("encode must succeed (encoder doesn't validate size)");

    // Decoder should reject oversized frame
    let result = codec.decode(&mut buf);
    assert!(
        result.is_err(),
        "decoder must reject frames exceeding max_frame_size"
    );
}

#[test]
fn codec_rejects_invalid_json() {
    let payload = b"not json";
    let len = (payload.len() as u32).to_le_bytes();
    let data: Vec<u8> = len.iter().chain(payload.iter()).copied().collect();

    let mut codec = IpcCodec::new();
    let mut buf = BytesMut::from(&data[..]);

    let result = codec.decode(&mut buf);
    assert!(result.is_err(), "invalid JSON must produce an error");
}
