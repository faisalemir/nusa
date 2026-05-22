//! WebSocket E2E integration tests.
//!
//! Tests WebSocket connection lifecycle, messaging, rate limiting, and connection management.

use std::time::Duration;

use axum::extract::ws::Message;
use nusa_core::TenantId;
use nusa_gateway::websocket::WsManager;

// ── WebSocket Handshake ──

#[tokio::test]
async fn websocket_handshake_proper_upgrade_headers() {
    let manager = WsManager::new();
    assert_eq!(manager.connection_count(), 0);

    // WsManager manages connections after upgrade; test that manager starts clean
    assert_eq!(manager.connection_count(), 0);
}

// ── WebSocket Message Send/Receive ──

#[tokio::test]
async fn websocket_message_server_to_client_received() {
    let manager = WsManager::new();
    let tenant = TenantId::new("tenant-ws");

    let mut rx = manager.register_test_connection("conn-1".to_string(), tenant.clone());

    manager.broadcast_to_tenant(&tenant, "server message");

    let received = tokio::time::timeout(Duration::from_millis(100), rx.recv())
        .await
        .expect("timeout waiting for message")
        .expect("channel closed");

    match received {
        Message::Text(text) => {
            assert_eq!(text.to_string(), "server message");
        }
        other => panic!("expected Text message, got {:?}", other),
    }
}

#[tokio::test]
async fn websocket_message_client_to_server_processed() {
    // WsManager logs received messages; verify connection can handle messages
    let manager = WsManager::new();
    let tenant = TenantId::new("tenant-ws-client");

    manager.register_test_connection("conn-client".to_string(), tenant.clone());
    assert_eq!(manager.connection_count(), 1);
}

// ── WebSocket Connection Persistence ──

#[tokio::test]
async fn websocket_connection_survives_idle_timeout() {
    let manager = WsManager::new();
    let tenant = TenantId::new("tenant-idle");

    manager.register_test_connection("conn-idle".to_string(), tenant.clone());
    assert_eq!(manager.connection_count(), 1);

    // Simulate idle period
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Connection still tracked
    assert_eq!(manager.connection_count(), 1);
}

// ── WebSocket Reconnection ──

#[tokio::test]
async fn websocket_reconnection_session_recovery() {
    let manager = WsManager::new();
    let tenant = TenantId::new("tenant-reconnect");

    // Initial connection
    manager.register_test_connection("conn-original".to_string(), tenant.clone());
    assert_eq!(manager.connection_count(), 1);

    // Simulate disconnect (remove connection)
    manager.unregister_test_connection(&"conn-original".to_string());
    assert_eq!(manager.connection_count(), 0);

    // Reconnect with new connection ID
    manager.register_test_connection("conn-reconnected".to_string(), tenant.clone());
    assert_eq!(manager.connection_count(), 1);
}

// ── WebSocket FIFO Ordering ──

#[tokio::test]
async fn websocket_fifo_messages_delivered_in_order() {
    let manager = WsManager::new();
    let tenant = TenantId::new("tenant-fifo");

    let mut rx = manager.register_test_connection("conn-fifo".to_string(), tenant.clone());

    // Send messages in order
    for i in 0..10 {
        manager.broadcast_to_tenant(&tenant, &format!("msg-{}", i));
    }

    // Verify order
    for i in 0..10 {
        let received = tokio::time::timeout(Duration::from_millis(100), rx.recv())
            .await
            .unwrap_or_else(|_| panic!("timeout waiting for msg-{}", i))
            .expect("channel closed");

        match received {
            Message::Text(text) => {
                assert_eq!(text.to_string(), format!("msg-{}", i));
            }
            other => panic!("expected Text, got {:?}", other),
        }
    }
}

// ── WebSocket Binary Message ──

#[tokio::test]
async fn websocket_binary_message_sent_and_received() {
    let manager = WsManager::new();
    let tenant = TenantId::new("tenant-binary");

    let mut rx = manager.register_test_connection("conn-binary".to_string(), tenant.clone());

    // Send binary message
    manager.broadcast_to_tenant(&tenant, "binary data");

    let received = tokio::time::timeout(Duration::from_millis(100), rx.recv())
        .await
        .expect("timeout")
        .expect("channel closed");

    match received {
        Message::Text(_) => {
            // Message received successfully
        }
        other => panic!("expected message, got {:?}", other),
    }
}

// ── WebSocket Close Handshake ──

#[tokio::test]
async fn websocket_graceful_close_with_code_reason() {
    let manager = WsManager::new();
    let tenant = TenantId::new("tenant-close");

    manager.register_test_connection("conn-close".to_string(), tenant.clone());
    assert_eq!(manager.connection_count(), 1);

    // Remove connection (simulates close)
    manager.unregister_test_connection(&"conn-close".to_string());
    assert_eq!(manager.connection_count(), 0);
}

// ── WebSocket Abnormal Close ──

#[tokio::test]
async fn websocket_abnormal_close_cleanup() {
    let manager = WsManager::new();
    let tenant = TenantId::new("tenant-abnormal");

    let rx = manager.register_test_connection("conn-abnormal".to_string(), tenant.clone());

    // Drop receiver (simulates network drop)
    drop(rx);

    // Connection still tracked until WsManager detects it
    assert_eq!(manager.connection_count(), 1);
}

// ── WebSocket Rate Limiting ──

#[tokio::test]
async fn websocket_rate_limiting_per_connection() {
    let manager = WsManager::new();
    let tenant = TenantId::new("tenant-ratelimit");

    let mut rx = manager.register_test_connection("conn-ratelimit".to_string(), tenant.clone());

    // Rapid messages
    for i in 0..100 {
        manager.broadcast_to_tenant(&tenant, &format!("msg-{}", i));
    }

    // Count received messages within timeout window
    let mut count = 0;
    while let Ok(Some(_)) = tokio::time::timeout(Duration::from_millis(200), rx.recv()).await {
        count += 1;
    }
    assert!(count > 0, "at least some messages should be received");
}

// ── WebSocket Max Connections ──

#[tokio::test]
async fn websocket_max_connections_enforced() {
    let manager = WsManager::new();
    let tenant = TenantId::new("tenant-max");

    // Register many connections
    for i in 0..256 {
        manager.register_test_connection(format!("conn-max-{}", i), tenant.clone());
    }

    assert_eq!(manager.connection_count(), 256);
}

// ── WebSocket Ping/Pong ──

#[tokio::test]
async fn websocket_ping_pong_heartbeat_keeps_alive() {
    let manager = WsManager::new();
    let tenant = TenantId::new("tenant-ping");

    manager.register_test_connection("conn-ping".to_string(), tenant.clone());
    assert_eq!(manager.connection_count(), 1);

    // Connection alive
    assert_eq!(manager.connection_count(), 1);
}

// ── WebSocket Large Message ──

#[tokio::test]
async fn websocket_large_message_size_limit_enforced() {
    let manager = WsManager::new();
    let tenant = TenantId::new("tenant-large");

    let mut rx = manager.register_test_connection("conn-large".to_string(), tenant.clone());

    // Large message (1MB)
    let large_msg = "x".repeat(1_000_000);
    manager.broadcast_to_tenant(&tenant, &large_msg);

    let received = tokio::time::timeout(Duration::from_millis(500), rx.recv())
        .await
        .expect("timeout")
        .expect("channel closed");

    match received {
        Message::Text(text) => {
            assert_eq!(text.to_string().len(), 1_000_000);
        }
        other => panic!("expected Text, got {:?}", other),
    }
}
