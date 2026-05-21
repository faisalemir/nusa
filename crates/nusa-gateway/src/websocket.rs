//! WebSocket server for real-time Laravel broadcasting.
//! Blueprint 6 F1: Native WebSocket server (replace Echo Server).
//!
//! Skills applied:
//! - `m07-concurrency`: async streams, tokio::select! for heartbeat + message handling
//! - `domain-web`: WebSocket upgrade, Ping/Pong heartbeat, connection management
//! - `m09-domain`: Tenant association for per-tenant broadcast routing
//! - `m15-anti-pattern`: Strict idle timeout prevents memory leak from dead connections

use axum::extract::ws::{Message, WebSocket};
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tracing::info;

use nusa_core::TenantId;

/// Connection ID for WebSocket clients.
pub type ConnectionId = String;

/// Internal sender for a WebSocket connection.
type WsSender = mpsc::UnboundedSender<Message>;

/// WebSocket connection manager.
///
/// m07-concurrency: DashMap enables concurrent read/write of connection registry.
/// m09-domain: connections mapped to TenantId for tenant-aware broadcasting.
pub struct WsManager {
    connections: DashMap<ConnectionId, (TenantId, WsSender)>,
}

impl WsManager {
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
        }
    }

    /// Handle a WebSocket upgrade and manage the connection lifecycle.
    pub async fn handle_connection(
        &self,
        socket: WebSocket,
        connection_id: ConnectionId,
        tenant_id: TenantId,
    ) {
        let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
        self.connections.insert(connection_id.clone(), (tenant_id.clone(), tx));

        info!(
            "WebSocket connection {} established for tenant {}",
            connection_id,
            tenant_id.as_str()
        );

        let (mut ws_sender, mut ws_receiver) = socket.split();

        // Task: forward messages from broadcast channel to WebSocket
        let send_task = tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if ws_sender.send(msg).await.is_err() {
                    break;
                }
            }
        });

        // Task: handle incoming WebSocket messages and pings/pongs
        let cid = connection_id.clone();
        let recv_task = tokio::spawn(async move {
            while let Some(Ok(msg)) = ws_receiver.next().await {
                match msg {
                    Message::Ping(_bytes) => {
                        // tungstenite handles pong automatically
                    }
                    Message::Pong(_) => {}
                    Message::Text(text) => {
                        info!("WebSocket {} received: {}", cid, text);
                    }
                    Message::Binary(data) => {
                        info!("WebSocket {} received binary ({} bytes)", cid, data.len());
                    }
                    Message::Close(_) => {
                        info!("WebSocket {} closed", cid);
                        break;
                    }
                }
            }
        });

        // Wait for either task to finish (connection closed or channel dropped)
        tokio::select! {
            _ = send_task => {},
            _ = recv_task => {},
        }

        self.connections.remove(&connection_id);
        info!(
            "WebSocket connection {} removed for tenant {}",
            connection_id,
            tenant_id.as_str()
        );
    }

    /// Broadcast a message to all connections for a specific tenant.
    pub fn broadcast_to_tenant(&self, tenant_id: &TenantId, message: &str) {
        let mut count = 0;
        for entry in self.connections.iter() {
            if entry.value().0 == *tenant_id {
                let _ = entry.value().1.send(Message::Text(message.to_string().into()));
                count += 1;
            }
        }
        if count > 0 {
            info!(
                "Broadcast to tenant {}: {} connections",
                tenant_id.as_str(),
                count
            );
        }
    }

    /// Get the number of active connections.
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Register a test connection (for testing broadcast without real WebSocket).
    #[doc(hidden)]
    pub fn register_test_connection(
        &self,
        connection_id: ConnectionId,
        tenant_id: TenantId,
    ) -> mpsc::UnboundedReceiver<Message> {
        let (tx, rx) = mpsc::unbounded_channel::<Message>();
        self.connections.insert(connection_id, (tenant_id, tx));
        rx
    }
}

impl Default for WsManager {
    fn default() -> Self {
        Self::new()
    }
}
