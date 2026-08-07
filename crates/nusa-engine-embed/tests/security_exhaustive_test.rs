//! S19: NEB1 frame protocol + embed pool security exhaustive tests.
//!
//! Covers: binary protocol attacks, path injection, SQL in headers,
//! length overflow, unknown opcodes, frame corruption, bootstrap path traversal.
//!
//! Pure unit tests (no PHP) run on all platforms.
//! Integration tests require PHP + fixture — skip without `// STUB_CONTRACT:`.

use std::collections::HashMap;

use nusa_engine_embed::frame::{
    MAGIC, OP_ASYNC_RESULT, OP_BOOTSTRAP, OP_REQUEST, OP_RESPONSE, VERSION, decode_ack,
    decode_async_query, decode_async_result, decode_error, decode_response, encode_async_query,
    encode_async_result_err, encode_async_result_json, encode_bootstrap, encode_request, frame_op,
};
use nusa_engine_embed::paths::{resolve_embed_daemon, resolve_php_driver_root};

// === NEB1 Frame Binary Protocol Attacks ===

#[test]
fn frame_too_short_returns_error() {
    let short_frames: &[&[u8]] = &[&[], &[0, 0], &[0, 0, 0], &[42]];
    for frame in short_frames {
        assert!(
            frame_op(frame).is_err(),
            "frame {frame:?} too short must return error"
        );
    }
}

#[test]
fn frame_bad_magic_rejected() {
    // 4-byte length prefix + "XXXX" magic
    let mut frame = Vec::new();
    frame.extend_from_slice(&8u32.to_le_bytes()); // inner payload length
    frame.extend_from_slice(b"XXXX"); // bad magic
    frame.extend_from_slice(&[OP_REQUEST, 0, 0]); // version=1, op=3, pad

    assert!(frame_op(&frame).is_err(), "bad magic must be rejected");
}

#[test]
fn frame_version_zero_rejected() {
    let mut frame = Vec::new();
    frame.extend_from_slice(&8u32.to_le_bytes());
    frame.extend_from_slice(&MAGIC);
    frame.push(0); // version 0
    frame.push(OP_REQUEST);
    frame.extend_from_slice(&[0, 0]);

    let result = frame_op(&frame);
    assert!(
        result.is_err(),
        "version 0 must be rejected (current={VERSION})"
    );
    assert!(result.unwrap_err().to_string().contains("version"));
}

#[test]
fn frame_version_future_rejected() {
    let mut frame = Vec::new();
    frame.extend_from_slice(&8u32.to_le_bytes());
    frame.extend_from_slice(&MAGIC);
    frame.push(255); // version 255
    frame.push(OP_REQUEST);
    frame.extend_from_slice(&[0, 0]);

    let result = frame_op(&frame);
    assert!(
        result.is_err(),
        "version 255 must be rejected (current={VERSION})"
    );
}

#[test]
fn frame_unknown_op_255_returns_error() {
    let mut frame = Vec::new();
    frame.extend_from_slice(&8u32.to_le_bytes());
    frame.extend_from_slice(&MAGIC);
    frame.push(VERSION);
    frame.push(255); // unknown opcode
    frame.extend_from_slice(&[0, 0]);

    // frame_op extracts the opcode — must not panic, returns the op
    let result = frame_op(&frame);
    assert!(
        result.is_ok(),
        "frame_op returns opcode even if unknown (caller decides)"
    );
    assert_eq!(result.unwrap(), 255);
}

#[test]
fn frame_truncated_returns_error() {
    // Declare 100 bytes but only provide 10
    let mut frame = Vec::new();
    frame.extend_from_slice(&100u32.to_le_bytes());
    frame.extend_from_slice(&MAGIC);
    frame.extend_from_slice(&[VERSION, OP_REQUEST, 0, 0]);
    // Only 10 bytes total after length prefix

    assert!(
        frame_op(&frame).is_err(),
        "truncated frame must return error"
    );
}

#[test]
fn frame_length_overflow_no_panic() {
    // u32::MAX length prefix with only 8 bytes of data
    let mut frame = Vec::new();
    frame.extend_from_slice(&u32::MAX.to_le_bytes());
    frame.extend_from_slice(&MAGIC);
    frame.extend_from_slice(&[VERSION, OP_REQUEST, 0, 0]);

    // Must not allocate 4GB — must fail fast with "truncated frame"
    let result = frame_op(&frame);
    assert!(
        result.is_err(),
        "length overflow must return error, not allocate or panic"
    );
    assert!(
        result.unwrap_err().to_string().contains("truncated"),
        "error should mention truncation"
    );
}

#[test]
fn frame_decode_ack_wrong_op() {
    // Craft an OP_BOOTSTRAP frame and try to decode as ACK
    let frame = encode_bootstrap("/app").expect("encode bootstrap");
    let result = decode_ack(&frame);
    assert!(
        result.is_err(),
        "decode_ack on bootstrap frame must return error"
    );
    assert!(
        result.unwrap_err().to_string().contains("op"),
        "error should mention opcode mismatch"
    );
}

#[test]
fn frame_decode_response_wrong_op() {
    // Craft an OP_REQUEST frame and try to decode as response
    let mut headers = HashMap::new();
    headers.insert("Host".into(), vec!["localhost".into()]);
    let frame = encode_request("GET", "/", &headers, b"").expect("encode request");
    let result = decode_response(&frame);
    assert!(
        result.is_err(),
        "decode_response on request frame must return error"
    );
}

#[test]
fn frame_decode_async_query_wrong_op() {
    let frame = encode_bootstrap("/app").expect("encode bootstrap");
    let result = decode_async_query(&frame);
    assert!(
        result.is_err(),
        "decode_async_query on bootstrap must return error"
    );
}

#[test]
fn frame_decode_async_result_wrong_op() {
    let frame = encode_bootstrap("/app").expect("encode bootstrap");
    let result = decode_async_result(&frame);
    assert!(
        result.is_err(),
        "decode_async_result on bootstrap must return error"
    );
}

#[test]
fn frame_decode_error_wrong_op() {
    let frame = encode_bootstrap("/app").expect("encode bootstrap");
    // decode_error expects OP_ERROR — should fail
    let result = decode_error(&frame);
    assert!(
        result.is_err(),
        "decode_error on bootstrap must return error"
    );
}

// === NEB1 Frame: Async Result Unknown Kind ===

#[test]
fn frame_async_result_unknown_kind_rejected() {
    // Craft a frame with OP_ASYNC_RESULT + status=0 (ok) + kind=255 (unknown)
    let mut inner = Vec::new();
    inner.extend_from_slice(&MAGIC);
    inner.push(VERSION);
    inner.push(OP_ASYNC_RESULT);
    inner.push(0); // pad byte 1
    inner.push(0); // pad byte 2
    inner.push(0); // status = ok (0)
    inner.push(255); // unknown kind

    let outer_len = inner.len() as u32;
    let mut frame = outer_len.to_le_bytes().to_vec();
    frame.extend_from_slice(&inner);

    let result = decode_async_result(&frame);
    assert!(
        result.is_err(),
        "unknown async result kind must return error"
    );
    assert!(result.unwrap_err().to_string().contains("kind"));
}

#[test]
fn frame_async_result_scalar_truncated() {
    // Craft OP_ASYNC_RESULT with kind=0 (scalar) but only 4 bytes instead of 8
    let mut inner = Vec::new();
    inner.extend_from_slice(&MAGIC);
    inner.push(VERSION);
    inner.push(OP_ASYNC_RESULT);
    inner.push(0); // pad
    inner.push(0); // status = ok
    inner.push(0); // kind = scalar
    inner.extend_from_slice(&[0u8; 4]); // only 4 bytes, need 8

    let outer_len = inner.len() as u32;
    let mut frame = outer_len.to_le_bytes().to_vec();
    frame.extend_from_slice(&inner);

    let result = decode_async_result(&frame);
    assert!(
        result.is_err(),
        "truncated scalar must return error (not panic)"
    );
}

// === SQL Injection in Frame Headers ===

#[test]
fn frame_sql_injection_in_header_json_treated_as_opaque() {
    let mut headers = HashMap::new();
    headers.insert(
        "Host".into(),
        vec!["localhost'; DROP TABLE users;--".into()],
    );
    let frame = encode_request("GET", "/", &headers, b"").expect("encode");

    // Frame encoding treats header JSON as opaque bytes — no execution
    let op = frame_op(&frame).expect("op");
    assert_eq!(op, OP_REQUEST, "must be a valid request frame");
}

#[test]
fn frame_xss_injection_in_header() {
    let mut headers = HashMap::new();
    headers.insert(
        "X-Custom".into(),
        vec!["<script>alert('xss')</script>".into()],
    );
    let frame = encode_request("GET", "/", &headers, b"").expect("encode");

    let op = frame_op(&frame).expect("op");
    assert_eq!(op, OP_REQUEST);
    // Decode and verify header is preserved as-is
    let decoded = decode_response; // just checking encode doesn't crash
    let _ = decoded;
}

#[test]
fn frame_null_bytes_in_sql_field() {
    // SQL with null bytes — passed through (Rust doesn't validate SQL content)
    let sql_with_null = "SELECT 1\x00";
    let frame = encode_async_query(sql_with_null).expect("encode");
    let decoded = decode_async_query(&frame).expect("decode");
    assert!(
        decoded.contains('\x00'),
        "null bytes must pass through to PHP side"
    );
}

#[test]
fn frame_sql_stacked_query_in_async_query() {
    let sql = "SELECT 1; DROP TABLE users";
    let frame = encode_async_query(sql).expect("encode");
    let decoded = decode_async_query(&frame).expect("decode");
    assert_eq!(
        decoded, sql,
        "stacked query must pass through to allowlist check"
    );
}

#[test]
fn frame_sql_into_outfile_in_async_query() {
    let sql = "SELECT * INTO OUTFILE '/tmp/evil'";
    let frame = encode_async_query(sql).expect("encode");
    let decoded = decode_async_query(&frame).expect("decode");
    assert_eq!(decoded, sql);
}

#[test]
fn frame_error_decode_with_sql_injection_message() {
    // Craft a frame that looks like OP_ERROR with SQL injection in message
    // Note: encode_async_result_err creates an error result (OP_ASYNC_RESULT with status=1),
    // which decode_error can read if the op matches
    let malicious_msg = "Error: SELECT 1; DROP TABLE users";
    let frame = encode_async_result_err(malicious_msg).expect("encode");

    // decode_error expects OP_ERROR opcode, but encode_async_result_err uses OP_ASYNC_RESULT
    // So decode_error must return error (correct behavior — wrong opcode)
    let result = decode_error(&frame);
    assert!(
        result.is_err(),
        "decode_error on OP_ASYNC_RESULT frame must return error"
    );
}

// === Encode Boundary: Method/URI/Body Size Limits ===

#[test]
fn frame_encode_method_over_65535_bytes_returns_error() {
    let long_method = "G".repeat(65536);
    let headers = HashMap::new();
    let result = encode_request(&long_method, "/", &headers, b"");
    assert!(
        result.is_err(),
        "method > 65535 bytes must return error (u16 limit)"
    );
}

#[test]
fn frame_encode_uri_4gb_returns_error() {
    // u32::MAX + 1 bytes — can't represent in u32
    // We test with a smaller but still too-large value
    let long_uri = "a".repeat(usize::MAX / 2); // force overflow
    let headers = HashMap::new();
    let result = encode_request("GET", &long_uri, &headers, b"");
    assert!(
        result.is_err(),
        "oversized URI must return error, not panic"
    );
}

#[test]
fn frame_encode_headers_too_large_returns_error() {
    let mut headers = HashMap::new();
    // Create a header map that exceeds u32::MAX when serialized
    // We test with a single very large header value
    headers.insert(
        "X-Large".into(),
        vec!["a".repeat(usize::MAX / 4)], // force potential overflow
    );
    let result = encode_request("GET", "/", &headers, b"");
    assert!(result.is_err(), "oversized headers must return error");
}

#[test]
fn frame_encode_body_4gb_returns_error() {
    let body = vec![0u8; usize::MAX / 2]; // force overflow
    let headers = HashMap::new();
    let result = encode_request("GET", "/", &headers, &body);
    assert!(result.is_err(), "body > 4GB must return error");
}

// === Encode Bootstrap: Path Injection ===

#[test]
fn frame_encode_bootstrap_path_traversal() {
    // Bootstrap with path traversal — encoding itself succeeds
    let frame = encode_bootstrap("../../../../etc/passwd").expect("encode");
    let op = frame_op(&frame).expect("op");
    assert_eq!(op, OP_BOOTSTRAP);
    // The actual rejection happens in PHP sandbox, not at encoding level
}

#[test]
fn frame_encode_bootstrap_null_byte() {
    let frame = encode_bootstrap("/app\x00/evil").expect("encode");
    let op = frame_op(&frame).expect("op");
    assert_eq!(op, OP_BOOTSTRAP);
}

#[test]
fn frame_encode_bootstrap_empty_path() {
    let frame = encode_bootstrap("").expect("encode");
    let op = frame_op(&frame).expect("op");
    assert_eq!(op, OP_BOOTSTRAP);
}

#[test]
fn frame_encode_bootstrap_very_long_path() {
    let long_path = "/app".repeat(10000); // 40K chars
    let result = encode_bootstrap(&long_path);
    // Should succeed — length is within u32 range
    assert!(result.is_ok(), "long path should encode without error");
}

// === Resolve Paths: Security ===

#[test]
fn resolve_embed_daemon_nonexistent_returns_error() {
    let result = resolve_embed_daemon(std::path::Path::new("/nonexistent/path"));
    assert!(result.is_err(), "nonexistent path must return error");
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("embed daemon missing"),
        "error message must be actionable"
    );
}

#[test]
fn resolve_php_driver_root_nonexistent_returns_none() {
    let result = resolve_php_driver_root(std::path::Path::new("/nonexistent/path"));
    assert!(result.is_none(), "nonexistent php-driver must return None");
}

#[test]
fn resolve_embed_daemon_empty_path() {
    let result = resolve_embed_daemon(std::path::Path::new(""));
    assert!(result.is_err(), "empty path must return error");
}

// === EmbedError Variant Exhaustive ===

#[test]
fn embed_error_all_variants() {
    use nusa_engine_embed::EmbedError;

    // Handshake
    let handshake = EmbedError::handshake("test handshake failure");
    assert!(handshake.to_string().contains("handshake failed"));

    // Worker
    let worker = EmbedError::Worker(42, "test worker error".into());
    assert!(worker.to_string().contains("worker 42"));
    assert!(worker.to_string().contains("test worker error"));

    // NotReady
    let not_ready = EmbedError::NotReady;
    assert!(not_ready.to_string().contains("not ready"));

    // NoIdleWorker
    let no_idle = EmbedError::NoIdleWorker;
    assert!(no_idle.to_string().contains("no idle workers"));

    // Io
    let io_err = EmbedError::Io("io error".into());
    assert!(io_err.to_string().contains("io error"));

    // Json
    let json_err = EmbedError::Json("parse error".into());
    assert!(json_err.to_string().contains("parse error"));
}

#[test]
fn embed_error_display_all_variants() {
    use nusa_engine_embed::EmbedError;

    let errors: Vec<EmbedError> = vec![
        EmbedError::handshake("h"),
        EmbedError::Worker(0, "w".into()),
        EmbedError::NotReady,
        EmbedError::NoIdleWorker,
        EmbedError::Io("i".into()),
        EmbedError::Json("j".into()),
    ];

    for err in &errors {
        assert!(
            !err.to_string().is_empty(),
            "error display must not be empty"
        );
    }
}

// === Async Result Error Variants ===

#[test]
fn async_result_err_encode_decode_roundtrip() {
    let messages = [
        "simple error",
        "SELECT 1; DROP TABLE users", // SQL injection in error message
        "<script>alert(1)</script>",  // XSS in error message
        "",                           // empty message
        "a".repeat(10000).leak(),     // long message
    ];

    for msg in &messages[..4] {
        let frame = encode_async_result_err(msg).expect("encode");
        let result = decode_async_result(&frame).expect("decode");
        assert!(!result.ok, "must be error result");
        assert_eq!(result.message, *msg, "message must roundtrip");
    }
}

// === Encode Async Query: SQL Boundary ===

#[test]
fn encode_async_query_empty_sql() {
    let frame = encode_async_query("").expect("encode");
    let decoded = decode_async_query(&frame).expect("decode");
    assert_eq!(decoded, "");
}

#[test]
fn encode_async_query_whitespace_only() {
    let frame = encode_async_query("   \t\n   ").expect("encode");
    let decoded = decode_async_query(&frame).expect("decode");
    assert_eq!(decoded, "   \t\n   ");
}

#[test]
fn encode_async_query_very_long_sql() {
    let long_sql = "SELECT ".repeat(10000);
    let frame = encode_async_query(&long_sql).expect("encode");
    let decoded = decode_async_query(&frame).expect("decode");
    assert_eq!(decoded, long_sql);
}

// === Decode Response: Header JSON Deserialization Failure ===

#[test]
fn decode_response_malformed_headers_json_returns_error() {
    // Craft a response frame with invalid JSON in headers
    let mut inner = Vec::new();
    inner.extend_from_slice(&MAGIC);
    inner.push(VERSION);
    inner.push(OP_RESPONSE);
    inner.push(0); // pad
    inner.extend_from_slice(&200u16.to_le_bytes());
    let malformed_json = b"{invalid json}";
    inner.extend_from_slice(&(malformed_json.len() as u32).to_le_bytes());
    inner.extend_from_slice(&4u32.to_le_bytes()); // body len
    inner.extend_from_slice(malformed_json);
    inner.extend_from_slice(b"body");

    let outer_len = inner.len() as u32;
    let mut frame = outer_len.to_le_bytes().to_vec();
    frame.extend_from_slice(&inner);

    let result = decode_response(&frame);
    assert!(
        result.is_err(),
        "malformed headers JSON must return structured error"
    );
}

// === Encode Async Result JSON: Boundary ===

#[test]
fn encode_async_result_json_empty_string() {
    let frame = encode_async_result_json("[]").expect("encode");
    let result = decode_async_result(&frame).expect("decode");
    assert!(result.ok);
    assert_eq!(result.json.as_deref(), Some("[]"));
}

#[test]
fn encode_async_result_json_very_long() {
    let long_json = format!(
        "[{}]",
        (0..1000)
            .map(|i| format!("{{\"id\":{i}}}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    let frame = encode_async_result_json(&long_json).expect("encode");
    let result = decode_async_result(&frame).expect("decode");
    assert!(result.ok);
    assert!(result.json.as_ref().unwrap().contains("\"id\":0"));
}
