//! WebSocket manager tests.
//! Tests WsManager connection lifecycle, broadcasting, and tenant isolation.

use std::time::Duration;

use axum::extract::ws::Message;
use nusa_core::TenantId;
use nusa_gateway::websocket::WsManager;

#[tokio::test]
async fn ws_manager_starts_empty() {
    let manager = WsManager::new();
    assert_eq!(manager.connection_count(), 0);
}

#[tokio::test]
async fn ws_manager_broadcast_to_empty_tenant() {
    let manager = WsManager::new();
    let tenant = TenantId::new("tenant-a");
    // Should not panic when no connections exist
    manager.broadcast_to_tenant(&tenant, "hello");
}

#[tokio::test]
async fn ws_manager_broadcast_reaches_correct_tenant() {
    let manager = WsManager::new();
    let tenant_a = TenantId::new("tenant-a");

    let mut rx_a = manager.register_test_connection("conn-1".to_string(), tenant_a.clone());

    manager.broadcast_to_tenant(&tenant_a, "message for A");

    let received = tokio::time::timeout(Duration::from_millis(100), rx_a.recv())
        .await
        .expect("timeout waiting for message")
        .expect("channel closed");

    match received {
        Message::Text(text) => {
            assert_eq!(text.to_string(), "message for A");
        }
        other => panic!("expected Text message, got {:?}", other),
    }
}

#[tokio::test]
async fn ws_manager_broadcast_isolation_between_tenants() {
    let manager = WsManager::new();
    let tenant_a = TenantId::new("tenant-a");
    let tenant_b = TenantId::new("tenant-b");

    let mut rx_a = manager.register_test_connection("conn-a".to_string(), tenant_a.clone());
    let mut rx_b = manager.register_test_connection("conn-b".to_string(), tenant_b.clone());

    // Broadcast to tenant-a only
    manager.broadcast_to_tenant(&tenant_a, "message for A");

    // Verify tenant-a receives
    let received_a = tokio::time::timeout(Duration::from_millis(100), rx_a.recv())
        .await
        .expect("timeout waiting for message from tenant-a")
        .expect("channel closed");
    assert!(matches!(received_a, Message::Text(ref t) if *t == "message for A"));

    // Verify tenant-b does NOT receive (timeout expected)
    let received_b = tokio::time::timeout(Duration::from_millis(100), rx_b.recv()).await;
    assert!(
        received_b.is_err(),
        "tenant-b should not receive message sent to tenant-a"
    );
}

#[tokio::test]
async fn ws_manager_broadcast_to_multiple_connections() {
    let manager = WsManager::new();
    let tenant = TenantId::new("tenant-multi");

    let mut receivers = Vec::new();
    for i in 0..5 {
        let rx = manager.register_test_connection(format!("conn-{}", i), tenant.clone());
        receivers.push(rx);
    }

    manager.broadcast_to_tenant(&tenant, "broadcast");

    for (i, rx) in receivers.iter_mut().enumerate() {
        let received = tokio::time::timeout(Duration::from_millis(100), rx.recv())
            .await
            .unwrap_or_else(|_| panic!("timeout waiting for connection {}", i))
            .expect("channel closed");
        assert!(matches!(received, Message::Text(ref t) if *t == "broadcast"));
    }
}

#[tokio::test]
async fn ws_manager_connection_count_tracks_connections() {
    let manager = WsManager::new();
    let tenant = TenantId::new("tenant-count");

    assert_eq!(manager.connection_count(), 0);

    for i in 0..3 {
        manager.register_test_connection(format!("conn-{}", i), tenant.clone());
    }

    assert_eq!(manager.connection_count(), 3);
}

#[tokio::test]
async fn ws_manager_default_implementation() {
    let manager = WsManager::default();
    assert_eq!(manager.connection_count(), 0);
}

#[tokio::test]
async fn ws_manager_concurrent_broadcasts() {
    let manager = std::sync::Arc::new(WsManager::new());
    let tenant = TenantId::new("tenant-concurrent");

    let mut rx = manager.register_test_connection("conn-concurrent".to_string(), tenant.clone());

    // 10 concurrent broadcasts
    let mut handles = Vec::new();
    for i in 0..10 {
        let m = manager.clone();
        let t = tenant.clone();
        handles.push(tokio::spawn(async move {
            m.broadcast_to_tenant(&t, &format!("msg-{}", i));
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // All 10 messages should have been sent
    let mut count = 0;
    while let Ok(Some(_)) = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
        count += 1;
    }
    assert_eq!(count, 10);
}

#[tokio::test]
async fn ws_manager_multiple_tenants_broadcast_independently() {
    let manager = WsManager::new();
    let tenants: Vec<_> = (0..5)
        .map(|i| TenantId::new(format!("tenant-{}", i)))
        .collect();

    let mut receivers = Vec::new();
    for (i, tenant) in tenants.iter().enumerate() {
        let rx = manager.register_test_connection(format!("conn-{}", i), tenant.clone());
        receivers.push(rx);
    }

    // Broadcast to each tenant independently
    for (i, tenant) in tenants.iter().enumerate() {
        manager.broadcast_to_tenant(tenant, &format!("msg-for-{}", i));
    }

    // Each receiver should get exactly its message
    for (i, rx) in receivers.iter_mut().enumerate() {
        let received = tokio::time::timeout(Duration::from_millis(100), rx.recv())
            .await
            .unwrap_or_else(|_| panic!("timeout for tenant {}", i))
            .expect("channel closed");
        assert!(matches!(received, Message::Text(ref t) if *t == format!("msg-for-{}", i)));
    }
}

#[tokio::test]
async fn ws_manager_connection_count_increments_correctly() {
    let manager = WsManager::new();
    let tenant = TenantId::new("tenant-inc");

    for i in 0..10 {
        manager.register_test_connection(format!("conn-{}", i), tenant.clone());
        assert_eq!(manager.connection_count(), i + 1);
    }
}
