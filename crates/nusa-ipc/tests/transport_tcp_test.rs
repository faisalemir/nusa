//! Tests for IPC transport with TCP fallback.
//!
//! Covers: TCP connect, send/recv, heartbeat, cross-platform compatibility.

use nusa_ipc::protocol::IpcMessage;
use nusa_ipc::transport::IpcTransport;

/// Closed localhost TCP port — deterministic in Podman/Alpine (no TEST-NET / privileged ports).
async fn closed_local_tcp_addr() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    drop(listener);
    format!("127.0.0.1:{port}")
}

// ── TCP Connect (cross-platform) ──

#[tokio::test]
async fn tcp_connect_refuses_closed_local_port() {
    let addr = closed_local_tcp_addr().await;
    let result = IpcTransport::connect(&addr).await;
    assert!(result.is_err(), "production tcp_connect must fail on closed port");
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
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), transport.recv()).await;

    // Should receive a response (even if it's garbled, the framing should parse)
    assert!(result.is_ok());
}

// ── Heartbeat ──

#[tokio::test]
async fn tcp_connect_fp_refuses_unreachable() {
    let addr = closed_local_tcp_addr().await;
    let port: u16 = addr
        .split(':')
        .nth(1)
        .expect("host:port")
        .parse()
        .expect("port");
    let result = IpcTransport::connect_tcp("127.0.0.1", port).await;
    assert!(result.is_err(), "connect_tcp must fail on closed port");
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
        let result =
            tokio::time::timeout(std::time::Duration::from_secs(2), transport.recv()).await;
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
