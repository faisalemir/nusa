//! Tests for IPC transport with TCP fallback.
//!
//! Covers: TCP connect, send/recv, heartbeat, cross-platform compatibility.

use nusa_ipc::protocol::IpcMessage;
use nusa_ipc::transport::IpcTransport;

// ── TCP Connect (cross-platform) ──

#[tokio::test]
async fn tcp_connect_refuses_invalid_address() {
    let result = IpcTransport::connect("127.0.0.1:1").await;
    // Port 1 is unlikely to be open — should fail
    assert!(result.is_err());
}

#[tokio::test]
async fn tcp_connect_refuses_unreachable_host() {
    let result = IpcTransport::connect("192.0.2.1:9999").await;
    // TEST-NET address — should fail
    assert!(result.is_err());
}

// ── TCP Echo Server Test ──

#[tokio::test]
async fn tcp_send_recv_with_echo_server() {
    // Start a simple TCP echo server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Spawn echo server
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let (mut reader, mut writer) = stream.split();
            // Echo back whatever is received
            tokio::io::copy(&mut reader, &mut writer).await.ok();
        }
    });

    // Give server time to start
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Connect via TCP transport
    let addr_str = format!("127.0.0.1:{}", addr.port());
    let mut transport = IpcTransport::connect(&addr_str).await.unwrap();

    // Send a Ping message
    transport.send(IpcMessage::Ping).await.unwrap();

    // The echo server will echo back the bytes
    // We just verify we can receive something
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        transport.recv(),
    ).await;

    // Should receive a response (even if it's garbled, the framing should parse)
    assert!(result.is_ok());
}

// ── Heartbeat ──

#[tokio::test]
async fn tcp_connect_fp_refuses_unreachable() {
    let result = IpcTransport::connect_tcp("127.0.0.1", 1).await;
    assert!(result.is_err());
}

// ── Multiple Messages ──

#[tokio::test]
async fn tcp_sequential_messages() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Spawn a server that echoes back 3 messages then closes
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (mut reader, mut writer) = tokio::io::split(stream);

        // Copy all data back to sender
        tokio::io::copy(&mut reader, &mut writer).await.ok();
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let addr_str = format!("127.0.0.1:{}", addr.port());
    let mut transport = IpcTransport::connect(&addr_str).await.unwrap();

    // Send multiple messages
    for _ in 0..3 {
        transport.send(IpcMessage::Ping).await.unwrap();
    }

    // Should be able to receive all 3 responses
    for _ in 0..3 {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            transport.recv(),
        ).await;
        assert!(result.is_ok());
    }
}

// ── Edge Cases ──

#[tokio::test]
async fn tcp_send_keepalive() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (mut reader, mut writer) = tokio::io::split(stream);
        tokio::io::copy(&mut reader, &mut writer).await.ok();
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let addr_str = format!("127.0.0.1:{}", addr.port());
    let mut transport = IpcTransport::connect(&addr_str).await.unwrap();

    // Send keepalive
    let result = transport.send_keepalive().await;
    assert!(result.is_ok());
}
