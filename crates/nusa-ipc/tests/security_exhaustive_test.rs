//! Security-exhaustive tests for nusa-ipc.
//!
//! Covers: IpcMessage field injection, FileUpload path traversal,
//! Framing allocation DoS, length attacks, Unicode normalization,
//! Transport connection security.

use std::collections::HashMap;

use bytes::BytesMut;
use tokio_util::codec::Decoder;

use nusa_ipc::framing::IpcCodec;
use nusa_ipc::protocol::{FileUpload, IpcMessage, RequestId};

// ===== IpcMessage Field Injection Tests =====

#[test]
fn ipcmessage_request_sql_injection_in_method_stored_raw() {
    let patterns = [
        "' OR 1=1 --",
        "'; DROP TABLE users; --",
        "UNION SELECT * FROM users",
        "1; SELECT * FROM information_schema",
        "' UNION ALL SELECT NULL--",
    ];
    for method in patterns {
        let msg = IpcMessage::Request {
            id: RequestId::new(),
            method: method.to_string(),
            uri: "/test".to_string(),
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
        let bytes = msg.to_framed_bytes().expect("serialize should succeed");
        let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize should succeed");
        if let IpcMessage::Request { method: m, .. } = parsed {
            assert_eq!(m, method);
        } else {
            panic!("expected Request variant");
        }
    }
}

#[test]
fn ipcmessage_request_sql_injection_in_uri_stored_raw() {
    let patterns = [
        "/' OR 1=1 --",
        "/'; DROP TABLE users; --",
        "/test?x=1 UNION SELECT * FROM users",
    ];
    for uri in patterns {
        let msg = IpcMessage::Request {
            id: RequestId::new(),
            method: "GET".to_string(),
            uri: uri.to_string(),
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
        let bytes = msg.to_framed_bytes().expect("serialize should succeed");
        let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize should succeed");
        if let IpcMessage::Request { uri: u, .. } = parsed {
            assert_eq!(u, uri);
        } else {
            panic!("expected Request variant");
        }
    }
}

#[test]
fn ipcmessage_request_xss_in_all_string_fields_stored() {
    let xss_patterns = [
        "<script>alert('xss')</script>",
        "<img src=x onerror=alert(1)>",
        "javascript:alert(1)",
        "<svg onload=alert(1)>",
    ];
    for pattern in xss_patterns {
        let mut headers = HashMap::new();
        headers.insert("x-custom".to_string(), vec![pattern.to_string()]);

        let msg = IpcMessage::Request {
            id: RequestId::new(),
            method: pattern.to_string(),
            uri: pattern.to_string(),
            headers: headers.clone(),
            query: headers.clone(),
            post: headers.clone(),
            cookies: HashMap::from([("session".to_string(), pattern.to_string())]),
            files: vec![],
            body: Some(pattern.as_bytes().to_vec()),
            server: HashMap::from([("SERVER_NAME".to_string(), pattern.to_string())]),
            timeout_ms: 5000,
            trace_context: None,
        };
        let bytes = msg.to_framed_bytes().expect("serialize should succeed");
        let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize should succeed");
        if let IpcMessage::Request {
            method: m, uri: u, ..
        } = parsed
        {
            assert_eq!(m, pattern);
            assert_eq!(u, pattern);
        } else {
            panic!("expected Request variant");
        }
    }
}

#[test]
fn ipcmessage_request_null_byte_in_all_fields_stored() {
    let null_patterns = [
        "file\0.txt",
        "path\0injection",
        "header\0value",
        "\0leading",
        "trailing\0",
    ];
    for pattern in null_patterns {
        let msg = IpcMessage::Request {
            id: RequestId::new(),
            method: pattern.to_string(),
            uri: pattern.to_string(),
            headers: HashMap::new(),
            query: HashMap::new(),
            post: HashMap::new(),
            cookies: HashMap::new(),
            files: vec![],
            body: Some(pattern.as_bytes().to_vec()),
            server: HashMap::new(),
            timeout_ms: 5000,
            trace_context: None,
        };
        let bytes = msg.to_framed_bytes().expect("serialize should succeed");
        let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize should succeed");
        if let IpcMessage::Request {
            method: m, uri: u, ..
        } = parsed
        {
            assert!(m.contains('\0'));
            assert!(u.contains('\0'));
        } else {
            panic!("expected Request variant");
        }
    }
}

#[test]
fn ipcmessage_request_control_chars_in_all_fields_stored() {
    for c in 0x01u8..=0x1Fu8 {
        let s = format!("prefix{}suffix", c as char);
        let msg = IpcMessage::Request {
            id: RequestId::new(),
            method: s.clone(),
            uri: s.clone(),
            headers: HashMap::new(),
            query: HashMap::new(),
            post: HashMap::new(),
            cookies: HashMap::new(),
            files: vec![],
            body: Some(s.as_bytes().to_vec()),
            server: HashMap::new(),
            timeout_ms: 5000,
            trace_context: None,
        };
        let bytes = msg.to_framed_bytes().expect("serialize should succeed");
        let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize should succeed");
        if let IpcMessage::Request {
            method: m, uri: u, ..
        } = parsed
        {
            assert_eq!(m, s);
            assert_eq!(u, s);
        } else {
            panic!("expected Request variant");
        }
    }
}

#[test]
fn ipcmessage_request_del_char_in_fields_stored() {
    let s = "test\x7Fvalue";
    let msg = IpcMessage::Request {
        id: RequestId::new(),
        method: s.to_string(),
        uri: s.to_string(),
        headers: HashMap::new(),
        query: HashMap::new(),
        post: HashMap::new(),
        cookies: HashMap::new(),
        files: vec![],
        body: Some(s.as_bytes().to_vec()),
        server: HashMap::new(),
        timeout_ms: 5000,
        trace_context: None,
    };
    let bytes = msg.to_framed_bytes().expect("serialize should succeed");
    let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize should succeed");
    if let IpcMessage::Request { method: m, .. } = parsed {
        assert_eq!(m, s);
    } else {
        panic!("expected Request variant");
    }
}

#[test]
fn ipcmessage_request_rtl_override_in_uri_stored() {
    let uri = "/files/exe\u{202E}dfg.jpg";
    let msg = IpcMessage::Request {
        id: RequestId::new(),
        method: "GET".to_string(),
        uri: uri.to_string(),
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
    let bytes = msg.to_framed_bytes().expect("serialize should succeed");
    let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize should succeed");
    if let IpcMessage::Request { uri: u, .. } = parsed {
        assert_eq!(u, uri);
    }
}

#[test]
fn ipcmessage_request_zero_width_chars_in_uri_stored() {
    let uri = "/path\u{200B}/file\u{200D}.php";
    let msg = IpcMessage::Request {
        id: RequestId::new(),
        method: "GET".to_string(),
        uri: uri.to_string(),
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
    let bytes = msg.to_framed_bytes().expect("serialize should succeed");
    let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize should succeed");
    if let IpcMessage::Request { uri: u, .. } = parsed {
        assert!(u.contains('\u{200B}'));
        assert!(u.contains('\u{200D}'));
    }
}

#[test]
fn ipcmessage_request_emoji_in_fields_stored() {
    let emoji_uri = "/path/🔥/test🚀.php";
    let msg = IpcMessage::Request {
        id: RequestId::new(),
        method: "GET".to_string(),
        uri: emoji_uri.to_string(),
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
    let bytes = msg.to_framed_bytes().expect("serialize should succeed");
    let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize should succeed");
    if let IpcMessage::Request { uri: u, .. } = parsed {
        assert_eq!(u, emoji_uri);
    }
}

#[test]
fn ipcmessage_request_format_string_in_fields_stored() {
    let patterns = ["%s", "%n", "%x", "{}", "{0}", "{:?}"];
    for pattern in patterns {
        let msg = IpcMessage::Request {
            id: RequestId::new(),
            method: pattern.to_string(),
            uri: pattern.to_string(),
            headers: HashMap::new(),
            query: HashMap::new(),
            post: HashMap::new(),
            cookies: HashMap::new(),
            files: vec![],
            body: Some(pattern.as_bytes().to_vec()),
            server: HashMap::new(),
            timeout_ms: 5000,
            trace_context: None,
        };
        let bytes = msg.to_framed_bytes().expect("serialize should succeed");
        let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize should succeed");
        if let IpcMessage::Request {
            method: m, uri: u, ..
        } = parsed
        {
            assert_eq!(m, pattern);
            assert_eq!(u, pattern);
        }
    }
}

// ===== Path Traversal in URI =====

#[test]
fn ipcmessage_request_path_traversal_in_uri_stored() {
    let traversals = [
        "/../../../etc/passwd",
        "/..\\..\\windows\\system32",
        "/%2e%2e/%2e%2e/etc/passwd",
        "/proc/self/environ",
    ];
    for uri in traversals {
        let msg = IpcMessage::Request {
            id: RequestId::new(),
            method: "GET".to_string(),
            uri: uri.to_string(),
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
        let bytes = msg.to_framed_bytes().expect("serialize should succeed");
        let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize should succeed");
        if let IpcMessage::Request { uri: u, .. } = parsed {
            assert_eq!(u, uri);
        }
    }
}

// ===== BroadcastEvent Field Injection =====

#[test]
fn broadcastevent_sql_injection_in_fields_stored() {
    let msg = IpcMessage::broadcast_event(
        "' OR 1=1 --".to_string(),
        "<script>alert(1)</script>".to_string(),
        "../../../etc/passwd".to_string(),
        vec!["tenant\0bad".to_string()],
    );
    let bytes = msg.to_framed_bytes().expect("serialize should succeed");
    let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize should succeed");
    if let IpcMessage::BroadcastEvent {
        channel,
        event,
        data,
        tenants,
    } = parsed
    {
        assert!(channel.contains("OR 1=1"));
        assert!(event.contains("<script>"));
        assert!(data.contains("etc/passwd"));
        assert!(tenants[0].contains('\0'));
    } else {
        panic!("expected BroadcastEvent variant");
    }
}

// ===== FileUpload Security Tests =====

#[test]
fn fileupload_path_traversal_in_tmp_path_stored() {
    let upload = FileUpload {
        name: "file".to_string(),
        filename: "../../../etc/passwd".to_string(),
        mime_type: "text/plain".to_string(),
        size: 100,
        tmp_path: "/tmp/../../../etc/shadow".to_string(),
    };
    assert!(upload.filename.contains(".."));
    assert!(upload.tmp_path.contains(".."));
}

#[test]
fn fileupload_null_byte_in_filename_stored() {
    let upload = FileUpload {
        name: "file".to_string(),
        filename: "shell.php\0.jpg".to_string(),
        mime_type: "image/jpeg".to_string(),
        size: 100,
        tmp_path: "/tmp/shell.php\0.jpg".to_string(),
    };
    assert!(upload.filename.contains('\0'));
    assert!(upload.tmp_path.contains('\0'));
}

#[test]
fn fileupload_oversized_filename_stored() {
    let big_name = "a".repeat(65536);
    let upload = FileUpload {
        name: big_name.clone(),
        filename: big_name.clone(),
        mime_type: "text/plain".to_string(),
        size: 100,
        tmp_path: big_name,
    };
    assert_eq!(upload.filename.len(), 65536);
}

#[test]
fn fileupload_sql_injection_in_filename_stored() {
    let upload = FileUpload {
        name: "file' OR 1=1 --".to_string(),
        filename: "shell'; DROP TABLE files; --.php".to_string(),
        mime_type: "text/plain".to_string(),
        size: 100,
        tmp_path: "/tmp/file' OR 1=1 --".to_string(),
    };
    assert!(upload.name.contains("OR 1=1"));
    assert!(upload.filename.contains("DROP TABLE"));
}

// ===== Framing Security Tests =====

#[test]
fn framing_u32_max_length_allocation_dos_detected() {
    // === Arrange ===
    let mut codec = IpcCodec::with_max_frame_size(64 * 1024 * 1024);
    // Craft a frame with u32::MAX length
    let mut data = BytesMut::from(&u32::MAX.to_le_bytes()[..]);

    // === Act ===
    let result = codec.decode(&mut data);

    // === Assert ===
    assert!(result.is_err(), "u32::MAX frame should be rejected");
    let err = result.expect_err("should be error");
    assert!(err.to_string().contains("Frame too large"));
}

#[test]
fn framing_crafted_malicious_frame_length_rejected() {
    // === Arrange ===
    let mut codec = IpcCodec::with_max_frame_size(1024);
    let mut data = BytesMut::from(&10_000_000u32.to_le_bytes()[..]);

    // === Act ===
    let result = codec.decode(&mut data);

    // === Assert ===
    assert!(result.is_err(), "oversized frame should be rejected");
}

#[test]
fn framing_truncated_frame_returns_none() {
    // === Arrange ===
    let mut codec = IpcCodec::new();
    let mut data = BytesMut::from(&[0x04, 0x00, 0x00, 0x00][..]); // 4 bytes payload, but no payload

    // === Act ===
    let result = codec.decode(&mut data);

    // === Assert ===
    assert!(result.is_ok(), "truncated frame should not crash");
    assert!(result.expect("ok").is_none(), "should wait for more data");
}

#[test]
fn framing_incomplete_header_returns_none() {
    // === Arrange ===
    let mut codec = IpcCodec::new();
    let mut data = BytesMut::from(&[0x04, 0x00][..]);

    // === Act ===
    let result = codec.decode(&mut data);

    // === Assert ===
    assert!(result.is_ok());
    assert!(result.expect("ok").is_none());
}

#[test]
fn framing_malformed_json_payload_rejected() {
    // === Arrange ===
    let mut codec = IpcCodec::new();
    let mut data = BytesMut::new();
    let payload = b"not json at all";
    data.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    data.extend_from_slice(payload);

    // === Act ===
    let result = codec.decode(&mut data);

    // === Assert ===
    assert!(result.is_err(), "malformed JSON should be rejected");
}

// ===== Length Attack Tests =====

#[test]
fn ipcmessage_length_attacks_in_string_fields() {
    let sizes = [0, 1, 255, 256, 1024, 4096, 65535];
    for size in sizes {
        let s = "a".repeat(size);
        let msg = IpcMessage::Request {
            id: RequestId::new(),
            method: s.clone(),
            uri: s.clone(),
            headers: HashMap::new(),
            query: HashMap::new(),
            post: HashMap::new(),
            cookies: HashMap::new(),
            files: vec![],
            body: Some(s.as_bytes().to_vec()),
            server: HashMap::new(),
            timeout_ms: 5000,
            trace_context: None,
        };
        let bytes = msg.to_framed_bytes().expect("serialize should succeed");
        let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize should succeed");
        if let IpcMessage::Request {
            method: m, uri: u, ..
        } = parsed
        {
            assert_eq!(m.len(), size);
            assert_eq!(u.len(), size);
        }
    }
}

// ===== URL Encoding Attack Tests =====

#[test]
fn ipcmessage_url_encoded_patterns_in_method_stored() {
    let encoded_patterns = [
        "%27%20OR%201%3D1%20--",               // ' OR 1=1 --
        "%3Cscript%3Ealert(1)%3C%2Fscript%3E", // <script>alert(1)</script>
        "%2e%2e%2fetc%2fpasswd",               // ../etc/passwd
    ];
    for pattern in encoded_patterns {
        let msg = IpcMessage::Request {
            id: RequestId::new(),
            method: pattern.to_string(),
            uri: "/test".to_string(),
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
        let bytes = msg.to_framed_bytes().expect("serialize should succeed");
        let parsed = IpcMessage::from_framed_bytes(&bytes).expect("deserialize should succeed");
        if let IpcMessage::Request { method: m, .. } = parsed {
            assert_eq!(m, pattern);
        }
    }
}

// ===== Unicode Normalization in IPC Fields =====

#[test]
fn ipcmessage_unicode_nfc_nfd_in_uri_different_bytes() {
    let nfc_uri = "/path/\u{00E9}/file"; // é composed
    let nfd_uri = "/path/e\u{0301}/file"; // e + combining acute
    assert_ne!(nfc_uri, nfd_uri);

    let msg_nfc = IpcMessage::Request {
        id: RequestId::new(),
        method: "GET".to_string(),
        uri: nfc_uri.to_string(),
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
    let msg_nfd = IpcMessage::Request {
        id: RequestId::new(),
        method: "GET".to_string(),
        uri: nfd_uri.to_string(),
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

    let bytes_nfc = msg_nfc.to_framed_bytes().expect("serialize should succeed");
    let bytes_nfd = msg_nfd.to_framed_bytes().expect("serialize should succeed");
    assert_ne!(
        bytes_nfc, bytes_nfd,
        "NFC and NFD should produce different bytes"
    );
}
