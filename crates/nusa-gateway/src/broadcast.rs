//! Redis Pub/Sub broadcast bridge for Laravel WebSocket events.
//! Blueprint 6 F3: Subscribe to Redis channels, push to WS/SSE connections.
//!
//! Skills applied:
//! - `m07-concurrency`: async redis pub/sub
//! - `domain-cloud-native`: External service integration for multi-node broadcast
//! - `m09-domain`: Channel-to-tenant mapping for per-tenant broadcast routing

use std::collections::HashMap;

use futures::StreamExt;
use redis::Client;
use tracing::{info, warn};

use nusa_core::TenantId;

/// Redis broadcast bridge manager.
///
/// m07-concurrency: Uses async redis connection for pub/sub.
/// domain-cloud-native: Integrates with Redis as external broadcast bus.
pub struct BroadcastBridge {
    client: Option<Client>,
    channel_map: HashMap<String, Vec<TenantId>>,
}

impl BroadcastBridge {
    /// Create a new broadcast bridge connected to Redis.
    pub async fn new(redis_url: &str) -> anyhow::Result<Self> {
        let client = Client::open(redis_url.to_string())?;
        info!("Redis broadcast bridge connected: {}", redis_url);
        Ok(Self {
            client: Some(client),
            channel_map: HashMap::new(),
        })
    }

    /// Stub bridge for single-node deployments.
    /// m15-anti-pattern: Graceful degradation when Redis is unavailable.
    pub fn stub() -> Self {
        Self {
            client: None,
            channel_map: HashMap::new(),
        }
    }

    /// Subscribe to a Redis channel and map it to a tenant (m09-domain).
    pub fn subscribe(&mut self, channel: String, tenant_id: TenantId) {
        self.channel_map.entry(channel).or_default().push(tenant_id);
    }

    /// Run the broadcast listener (m07-concurrency).
    ///
    /// Subscribes to all registered channels and forwards messages
    /// to the appropriate WebSocket/SSE connections.
    pub async fn run_listener(
        self,
        ws_manager: &crate::websocket::WsManager,
        sse_manager: &crate::sse::SseManager,
    ) -> anyhow::Result<()> {
        if let Some(client) = self.client {
            // Subscribe to all registered channels
            let channels: Vec<&str> = self.channel_map.keys().map(|s| s.as_str()).collect();
            if channels.is_empty() {
                warn!("No channels registered, starting in idle mode");
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                }
            }

            info!(
                "Redis broadcast listener subscribing to {} channels",
                channels.len()
            );

            let mut pubsub = client.get_async_pubsub().await?;
            for channel in &channels {
                pubsub.subscribe(*channel).await?;
            }

            info!("Redis broadcast listener ready");

            // Process incoming messages
            let channel_map = self.channel_map;
            while let Some(msg) = pubsub.on_message().next().await {
                let channel = msg.get_channel_name().to_string();
                let payload: String = msg.get_payload()?;

                info!(
                    channel,
                    "Redis broadcast received message ({} bytes)",
                    payload.len()
                );

                // Forward to tenants subscribed to this channel
                if let Some(tenants) = channel_map.get(&channel) {
                    for tenant_id in tenants {
                        ws_manager.broadcast_to_tenant(tenant_id, &payload);
                        sse_manager.send(&payload);
                    }
                }
            }

            Ok(())
        } else {
            warn!("Broadcast bridge is in stub mode (no Redis connection)");
            // Stub: just keep alive
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            }
        }
    }
}
