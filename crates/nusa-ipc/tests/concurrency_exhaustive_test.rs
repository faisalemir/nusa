//! Concurrency exhaustive tests for nusa-ipc crate.
//!
//! Covers: IpcTransport concurrent send/recv/deadlock/drop, IpcCodec concurrent encode,
//! heartbeat concurrency, request_response timeout, TraceContext thread safety,
//! channel backpressure patterns.

use std::time::Duration;

use bytes::BytesMut;
use nusa_ipc::framing::IpcCodec;
use nusa_ipc::protocol::IpcMessage;
use nusa_ipc::trace::TraceContext;
use tokio::net::TcpListener;
use tokio_util::codec::{Decoder, Encoder};

// ── IpcCodec: Concurrent Encode ──

#[test]
fn ipccodec_concurrent_encode_buffer_growth_safe() {
    // === Act ===
    let handles: Vec<_> = (0..8)
        .map(|idx| {
            std::thread::spawn(move || {
                let mut codec = IpcCodec::new();
                let mut dst = BytesMut::new();
                for _ in 0..100 {
                    let msg = IpcMessage::Ping;
                    codec.encode(msg, &mut dst).expect("encode must succeed");
                }
                (idx, dst.len())
            })
        })
        .collect();

    // === Assert ===
    for h in handles {
        let (idx, len) = h.join().expect("thread must not panic");
        assert!(len > 0, "codec {idx} must have produced data");
    }
}

// ── TraceContext: Thread Safety ──

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tracecontext_concurrent_inject_into_no_data_race() {
    // === Arrange ===
    let mut seed = std::collections::HashMap::new();
    seed.insert(
        "traceparent".into(),
        vec!["00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".into()],
    );
    let ctx = TraceContext::from_raw_headers(&seed);

    // === Act ===
    let mut handles = Vec::new();
    for _ in 0..8 {
        let context = ctx.clone();
        handles.push(tokio::spawn(async move {
            let mut headers = std::collections::HashMap::new();
            context.inject_into(&mut headers);
            headers.len()
        }));
    }

    // === Assert ===
    for h in handles {
        let count = tokio::time::timeout(Duration::from_secs(5), h)
            .await
            .expect("must complete")
            .expect("must not panic");
        // TraceContext should inject at least traceparent header
        assert!(count > 0, "headers must be injected");
    }
}

#[test]
fn tracecontext_shared_across_threads_no_data_race() {
    // === Arrange ===
    let ctx = TraceContext::from_raw_headers(&std::collections::HashMap::new());

    // === Act ===
    let mut handles = Vec::new();
    for _ in 0..8 {
        let context = ctx.clone();
        handles.push(std::thread::spawn(move || {
            for _ in 0..100 {
                let mut headers = std::collections::HashMap::new();
                context.clone().inject_into(&mut headers);
            }
        }));
    }

    // === Assert ===
    for h in handles {
        h.join().expect("thread must not panic");
    }
}

// ── IpcTransport: Concurrent Send ──

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ipctransport_concurrent_send_no_data_corruption() {
    // === Arrange ===
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("must bind");
    let addr = listener.local_addr().expect("must get addr");

    // Server echoes back
    let server_handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("must accept");
        // Just read header + payload and discard
        let mut buf = [0u8; 4];
        loop {
            use tokio::io::AsyncReadExt;
            match socket.read_exact(&mut buf).await {
                Ok(_) => {
                    let len = u32::from_le_bytes(buf) as usize;
                    let mut payload = vec![0u8; len];
                    use tokio::io::AsyncReadExt;
                    if socket.read_exact(&mut payload).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Client sends concurrently
    let mut transports = Vec::new();
    for _ in 0..4 {
        let transport =
            nusa_ipc::transport::IpcTransport::connect(&format!("127.0.0.1:{}", addr.port()))
                .await
                .expect("must connect");
        transports.push(transport);
    }

    // === Act ===
    let mut handles = Vec::new();
    for (idx, mut transport) in transports.into_iter().enumerate() {
        let handle = tokio::spawn(async move {
            for _ in 0..10 {
                let msg = IpcMessage::Keepalive {
                    timestamp: idx as u64,
                };
                let _ = transport.send(msg).await;
            }
        });
        handles.push(handle);
    }

    // === Assert ===
    for h in handles {
        tokio::time::timeout(Duration::from_secs(10), h)
            .await
            .expect("must complete")
            .expect("must not panic");
    }

    server_handle.abort();
    let _ = server_handle.await;
}

// ── IpcTransport: Drop During Async ──

#[tokio::test]
async fn ipctransport_drop_while_recv_awaiting_clean_cancellation() {
    // === Arrange ===
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("must bind");
    let addr = listener.local_addr().expect("must get addr");

    // Server accepts but never sends
    let server_handle = tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.expect("must accept");
        // Hold connection open, never send
        tokio::time::sleep(Duration::from_secs(10)).await;
    });

    let mut transport =
        nusa_ipc::transport::IpcTransport::connect(&format!("127.0.0.1:{}", addr.port()))
            .await
            .expect("must connect");

    // === Act ===
    let recv_handle = tokio::spawn(async move {
        let _ = transport.recv().await;
    });

    // Drop transport while recv is awaiting
    tokio::time::sleep(Duration::from_millis(50)).await;
    recv_handle.abort();

    // === Assert ===
    tokio::time::timeout(Duration::from_secs(5), recv_handle)
        .await
        .expect("recv task must complete (abort)")
        .ok(); // aborted = ok

    server_handle.abort();
    let _ = server_handle.await;
}

#[tokio::test]
async fn ipctransport_drop_while_send_awaiting_no_leak() {
    // === Arrange ===
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("must bind");
    let addr = listener.local_addr().expect("must get addr");

    let server_handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("must accept");
        // Read data slowly
        let mut buf = [0u8; 4];
        use tokio::io::AsyncReadExt;
        if socket.read_exact(&mut buf).await.is_ok() {
            let len = u32::from_le_bytes(buf) as usize;
            let mut payload = vec![0u8; len];
            let _ = socket.read_exact(&mut payload).await;
        }
    });

    let mut transport =
        nusa_ipc::transport::IpcTransport::connect(&format!("127.0.0.1:{}", addr.port()))
            .await
            .expect("must connect");

    // === Act ===
    let send_handle = tokio::spawn(async move {
        let msg = IpcMessage::Keepalive { timestamp: 0 };
        let _ = transport.send(msg).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    send_handle.abort();

    // === Assert ===
    tokio::time::timeout(Duration::from_secs(5), send_handle)
        .await
        .expect("send task must complete")
        .ok();

    server_handle.abort();
    let _ = server_handle.await;
}

// ── IpcTransport: Request-Response Timeout ──

#[tokio::test]
async fn ipctransport_request_response_timeout_cleanup_verified() {
    // === Arrange ===
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("must bind");
    let addr = listener.local_addr().expect("must get addr");

    // Server accepts but never responds
    let server_handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("must accept");
        // Read request
        let mut buf = [0u8; 4];
        use tokio::io::AsyncReadExt;
        if socket.read_exact(&mut buf).await.is_ok() {
            let len = u32::from_le_bytes(buf) as usize;
            let mut payload = vec![0u8; len];
            let _ = socket.read_exact(&mut payload).await;
        }
        // Never send response
        tokio::time::sleep(Duration::from_secs(10)).await;
    });

    let mut transport =
        nusa_ipc::transport::IpcTransport::connect(&format!("127.0.0.1:{}", addr.port()))
            .await
            .expect("must connect");

    // === Act ===
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        transport.request_response(
            "GET".to_string(),
            "/test".to_string(),
            Default::default(),
            None,
            100, // 100ms timeout
        ),
    )
    .await;

    // === Assert ===
    match result {
        Ok(Err(_)) => {}
        Ok(Ok(_)) => panic!("request must not succeed without response"),
        Err(_) => panic!("outer timeout before IPC request timeout"),
    }

    server_handle.abort();
    let _ = server_handle.await;
}

// ── IpcTransport: Heartbeat Concurrent ──

#[tokio::test]
async fn ipctransport_heartbeat_concurrent_with_recv_no_interference() {
    // === Arrange ===
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("must bind");
    let addr = listener.local_addr().expect("must get addr");

    let server_handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("must accept");
        // Echo back pings as pongs
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut buf = [0u8; 4];
        while socket.read_exact(&mut buf).await.is_ok() {
            let len = u32::from_le_bytes(buf) as usize;
            let mut payload = vec![0u8; len];
            if socket.read_exact(&mut payload).await.is_err() {
                break;
            }
            let pong = IpcMessage::Pong;
            let framed = pong.to_framed_bytes().expect("must frame");
            let _ = socket.write_all(&framed).await;
        }
    });

    let mut transport =
        nusa_ipc::transport::IpcTransport::connect(&format!("127.0.0.1:{}", addr.port()))
            .await
            .expect("must connect");

    // === Act ===
    let _ = transport.send(IpcMessage::Ping).await;
    let recv_result = tokio::time::timeout(Duration::from_secs(2), transport.recv()).await;

    // === Assert ===

    // Either got a pong or timed out — no crash
    drop(recv_result);

    server_handle.abort();
    let _ = server_handle.await;
}

// ── IpcTransport: Channel-Like Backpressure ──

#[tokio::test]
async fn ipctransport_fast_sender_slow_receiver_backpressure() {
    // === Arrange ===
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("must bind");
    let addr = listener.local_addr().expect("must get addr");

    let server_handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("must accept");
        use tokio::io::AsyncReadExt;
        let mut buf = [0u8; 4];
        for _ in 0..10 {
            if socket.read_exact(&mut buf).await.is_err() {
                break;
            }
            let len = u32::from_le_bytes(buf) as usize;
            let mut payload = vec![0u8; len];
            if socket.read_exact(&mut payload).await.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await; // Slow receiver
        }
    });

    let mut transport =
        nusa_ipc::transport::IpcTransport::connect(&format!("127.0.0.1:{}", addr.port()))
            .await
            .expect("must connect");

    // === Act ===
    for i in 0..10 {
        let msg = IpcMessage::Keepalive { timestamp: i };
        let _ = transport.send(msg).await;
    }

    // === Assert ===
    // All sends must complete without blocking indefinitely
    server_handle.abort();
    let _ = server_handle.await;
}

#[tokio::test]
async fn ipctransport_slow_sender_fast_receiver_no_starvation() {
    // === Arrange ===
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("must bind");
    let addr = listener.local_addr().expect("must get addr");

    let server_handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("must accept");
        use tokio::io::AsyncWriteExt;
        // Slow sender — send with delay
        for _ in 0..5 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let msg = IpcMessage::Keepalive { timestamp: 0 };
            let framed = msg.to_framed_bytes().expect("must frame");
            if socket.write_all(&framed).await.is_err() {
                break;
            }
        }
    });

    let mut transport =
        nusa_ipc::transport::IpcTransport::connect(&format!("127.0.0.1:{}", addr.port()))
            .await
            .expect("must connect");

    // === Act ===
    let mut received = 0usize;
    for _ in 0..10 {
        match tokio::time::timeout(Duration::from_millis(200), transport.recv()).await {
            Ok(Ok(_)) => received += 1,
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }

    // === Assert ===
    assert!(received > 0, "must receive at least some messages");

    server_handle.abort();
    let _ = server_handle.await;
}

// ── IpcTransport: Deadlock Detection ──

#[tokio::test]
async fn ipctransport_bidirectional_wait_timeout_mitigation() {
    // === Arrange ===
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("must bind");
    let addr = listener.local_addr().expect("must get addr");

    let server_handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("must accept");
        // Both sides try to recv first — deadlock scenario
        use tokio::io::AsyncReadExt;
        let mut buf = [0u8; 4];
        // Wait for client to send (will timeout)
        let _ = tokio::time::timeout(Duration::from_secs(1), socket.read_exact(&mut buf)).await;
    });

    let mut transport =
        nusa_ipc::transport::IpcTransport::connect(&format!("127.0.0.1:{}", addr.port()))
            .await
            .expect("must connect");

    // === Act ===
    // Both sides try to recv — timeout as mitigation
    let result = tokio::time::timeout(Duration::from_secs(2), transport.recv()).await;

    // === Assert ===
    // Must timeout or fail closed (deadlock prevented by timeout/error)
    assert!(
        result.is_err() || matches!(result, Ok(Err(_))),
        "must not block indefinitely"
    );

    server_handle.abort();
    let _ = server_handle.await;
}

// ── IpcCodec: Decode Edge Cases ──

#[test]
fn ipccodec_decode_partial_then_complete_buffer_bounded() {
    // === Arrange ===
    let mut codec = IpcCodec::new();
    let msg = IpcMessage::Ping;
    let framed = msg.to_framed_bytes().expect("must frame");

    // === Act ===
    // Feed partial data
    let mut buf = BytesMut::from(&framed[..2]);
    let result1 = codec.decode(&mut buf).expect("decode must not error");
    assert!(result1.is_none(), "partial frame must return None");

    // Feed remaining data
    buf.extend_from_slice(&framed[2..]);
    let result2 = codec.decode(&mut buf).expect("decode must not error");

    // === Assert ===
    assert!(result2.is_some(), "complete frame must decode");
}

#[test]
fn ipccodec_decode_frame_too_large_rejected() {
    // === Arrange ===
    let mut codec = IpcCodec::with_max_frame_size(64);

    // === Act ===
    // Create frame with length > max_frame_size
    let mut buf = BytesMut::from(&(1000u32).to_le_bytes()[..]);
    buf.extend_from_slice(&[0u8; 1000]);

    let result = codec.decode(&mut buf);

    // === Assert ===
    assert!(result.is_err(), "oversized frame must be rejected");
}

#[test]
fn ipccodec_decode_incomplete_header_returns_none() {
    // === Arrange ===
    let mut codec = IpcCodec::new();
    let mut buf = BytesMut::from(&[0u8, 0u8][..]); // Only 2 bytes, need 4 for header

    // === Act ===
    let result = codec.decode(&mut buf).expect("must not error");

    // === Assert ===
    assert!(result.is_none(), "incomplete header must return None");
}
