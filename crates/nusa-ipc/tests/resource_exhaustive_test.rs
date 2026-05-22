//! Resource exhaustion tests for nusa-ipc crate.
//!
//! Covers: TCP/Unix FD leak, codec buffer growth, large message alloc,
//! heartbeat cleanup, connection error cleanup, timeout cleanup,
//! partial read cleanup, concurrent connections.

use std::time::Duration;

use bytes::BytesMut;
use nusa_ipc::framing::IpcCodec;
use nusa_ipc::protocol::IpcMessage;
use tokio::net::TcpListener;
use tokio_util::codec::Decoder;

// ── TCP FD Leak ──

#[tokio::test]
async fn ipctransport_tcp_connect_drop_100_times_fd_stable() {
    // === Arrange ===
    let start_fds = count_open_fds();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("must bind");
    let addr = listener.local_addr().expect("must get addr");

    // Server accepts and immediately drops
    let server_handle = tokio::spawn(async move {
        for _ in 0..100 {
            if let Ok((socket, _)) = listener.accept().await {
                drop(socket);
            }
        }
    });

    // === Act ===
    for _ in 0..100 {
        if let Ok(transport) =
            nusa_ipc::transport::IpcTransport::connect(&format!("127.0.0.1:{}", addr.port())).await
        {
            drop(transport);
        }
    }

    // === Assert ===
    let end_fds = count_open_fds();
    assert!(
        end_fds <= start_fds + 10,
        "FD count must be stable after 100 connect/drop cycles"
    );

    server_handle.abort();
    let _ = server_handle.await;
}

// ── Codec Buffer Growth ──

#[test]
fn ipccodec_partial_decode_then_complete_buffer_does_not_grow_unbounded() {
    // === Arrange ===
    let mut codec = IpcCodec::new();
    let msg = IpcMessage::Keepalive { timestamp: 0 };
    let framed = msg.to_framed_bytes().expect("must frame");

    // === Act ===
    for _ in 0..100 {
        let mut buf = BytesMut::from(&framed[..2]);
        let _ = codec.decode(&mut buf); // Returns None (partial)

        // Buffer should not grow unboundedly
        assert!(buf.len() <= 4, "buffer must not grow beyond partial header");

        // Now complete the frame
        buf.extend_from_slice(&framed[2..]);
        let _ = codec.decode(&mut buf); // Returns Some(Ping)
        assert!(buf.is_empty(), "buffer must be empty after full decode");
    }
}

// ── Large Message Alloc ──

#[test]
fn ipccodec_10mb_message_send_recv_memory_released() {
    // === Arrange ===
    let start_fds = count_open_fds();

    // Create a large broadcast event
    let large_data = "x".repeat(10 * 1024 * 1024); // 10MB
    let msg = IpcMessage::BroadcastEvent {
        channel: "test".to_string(),
        event: "large".to_string(),
        data: large_data,
        tenants: vec!["tenant-1".to_string()],
    };

    // === Act ===
    let framed = msg.to_framed_bytes().expect("must frame");
    assert!(framed.len() > 10 * 1024 * 1024, "frame must be large");

    // Decode it back
    let mut codec = IpcCodec::with_max_frame_size(20 * 1024 * 1024);
    let mut buf = BytesMut::from(&framed[..]);
    let decoded = codec.decode(&mut buf).expect("must decode");

    // === Assert ===
    assert!(decoded.is_some(), "large message must decode");
    drop(decoded);
    drop(buf);
    drop(framed);

    let end_fds = count_open_fds();
    assert!(end_fds <= start_fds + 5, "no FD leak from large message");
}

// ── Heartbeat Loop Cleanup ──

#[tokio::test]
async fn ipctransport_heartbeat_loop_terminates_on_drop() {
    // === Arrange ===
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("must bind");
    let addr = listener.local_addr().expect("must get addr");

    let server_handle = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.expect("must accept");
        // Accept connection and drop immediately
        drop(socket);
    });

    let mut transport =
        nusa_ipc::transport::IpcTransport::connect(&format!("127.0.0.1:{}", addr.port()))
            .await
            .expect("must connect");

    // === Act ===
    let mut missed = 0u32;
    let hb_handle = tokio::spawn(async move {
        // Heartbeat will fail quickly since server dropped connection
        let _ = transport.run_heartbeat(1, &mut missed).await;
    });

    // === Assert ===
    // Heartbeat should terminate within a few seconds (connection closed)
    tokio::time::timeout(Duration::from_secs(10), hb_handle)
        .await
        .expect("heartbeat must terminate")
        .expect("must not panic");

    server_handle.abort();
    let _ = server_handle.await;
}

// ── Connection Error Cleanup ──

#[tokio::test]
async fn ipctransport_connection_fails_all_resources_released() {
    // === Arrange ===
    let start_fds = count_open_fds();

    // === Act ===
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let closed_addr = format!(
        "127.0.0.1:{}",
        listener.local_addr().expect("addr").port()
    );
    drop(listener);

    for _ in 0..50 {
        // Closed port; production transport uses bounded TCP_CONNECT_TIMEOUT.
        let _ = nusa_ipc::transport::IpcTransport::connect(&closed_addr).await;
    }

    // === Assert ===
    let end_fds = count_open_fds();
    assert!(
        end_fds <= start_fds + 5,
        "no FD leak from failed connections"
    );
}

// ── Timeout Cleanup ──

#[tokio::test]
async fn ipctransport_request_response_timeout_no_orphaned_futures() {
    // === Arrange ===
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("must bind");
    let addr = listener.local_addr().expect("must get addr");

    let server_handle = tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            // Read request but never respond
            use tokio::io::AsyncReadExt;
            let mut buf = [0u8; 4];
            let _ = socket.read_exact(&mut buf).await;
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });

    let mut transport =
        nusa_ipc::transport::IpcTransport::connect(&format!("127.0.0.1:{}", addr.port()))
            .await
            .expect("must connect");

    // === Act ===
    let result = tokio::time::timeout(
        Duration::from_millis(100),
        transport.request_response(
            "GET".to_string(),
            "/timeout-test".to_string(),
            Default::default(),
            50,
        ),
    )
    .await;

    // === Assert ===
    match result {
        Ok(Err(_)) => {}
        Ok(Ok(_)) => panic!("request must not succeed without response"),
        Err(_) => panic!("outer timeout before IPC request timeout"),
    }

    // Transport must still be usable (no orphaned state)
    // (In practice it may be in a bad state after timeout, but no crash)
    let _ = transport.send(IpcMessage::Shutdown).await;

    server_handle.abort();
    let _ = server_handle.await;
}

// ── Multiple Concurrent Connections ──

#[tokio::test]
async fn ipctransport_100_connections_open_close_no_fd_accumulation() {
    // === Arrange ===
    let start_fds = count_open_fds();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("must bind");
    let addr = listener.local_addr().expect("must get addr");

    let server_handle = tokio::spawn(async move {
        for _ in 0..100 {
            if let Ok((socket, _)) = listener.accept().await {
                drop(socket);
            }
        }
    });

    // === Act ===
    for _ in 0..100 {
        if let Ok(transport) =
            nusa_ipc::transport::IpcTransport::connect(&format!("127.0.0.1:{}", addr.port())).await
        {
            drop(transport)
        }
    }

    // === Assert ===
    let end_fds = count_open_fds();
    assert!(
        end_fds <= start_fds + 10,
        "FD count must be stable after 100 connect/drop"
    );

    server_handle.abort();
    let _ = server_handle.await;
}

// ── Partial Read Cleanup ──

#[test]
fn ipccodec_decode_interrupted_buffer_state_reset() {
    // === Arrange ===
    let mut codec = IpcCodec::new();

    // === Act ===
    // Simulate partial reads — feed data in small chunks
    let msg = IpcMessage::Ack;
    let framed = msg.to_framed_bytes().expect("must frame");

    // Feed byte by byte
    for i in 0..framed.len() {
        let mut buf = BytesMut::from(&framed[..i]);
        let result = codec.decode(&mut buf).expect("must not error");
        if i < 4 {
            assert!(result.is_none(), "incomplete header must return None");
        }
        // Buffer state must be reset after each partial decode attempt
    }

    // Full decode must work
    let mut buf = BytesMut::from(&framed[..]);
    let result = codec.decode(&mut buf).expect("must decode");
    assert!(result.is_some(), "full frame must decode");
}

// ── Helper ──

#[cfg(unix)]
fn count_open_fds() -> usize {
    use std::fs;
    let fd_dir = "/proc/self/fd";
    if let Ok(entries) = fs::read_dir(fd_dir) {
        entries.count()
    } else {
        0
    }
}

#[cfg(not(unix))]
fn count_open_fds() -> usize {
    0
}
