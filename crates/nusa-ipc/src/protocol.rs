//! IPC protocol messages and framing.
//!
//! Skills applied:
//! - `m06-error-handling`: serde errors propagate as Results
//! - `m11-ecosystem`: serde + rmp-serde for cross-platform serialization

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::trace::TraceContext;

/// JSON body field: PHP workers send UTF-8 strings; Rust tests may use byte arrays.
mod json_body {
    use serde::{Deserializer, Serializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BodyVisitor;

        impl<'de> serde::de::Visitor<'de> for BodyVisitor {
            type Value = Vec<u8>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a byte array or UTF-8 string")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(v.as_bytes().to_vec())
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E> {
                Ok(v.to_vec())
            }

            fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E> {
                Ok(v)
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut bytes = Vec::new();
                while let Some(b) = seq.next_element::<u8>()? {
                    bytes.push(b);
                }
                Ok(bytes)
            }
        }

        deserializer.deserialize_any(BodyVisitor)
    }

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_seq(bytes.iter())
    }
}

/// Unique ID for each request/task to correlate responses
///
/// # Example
/// ```rust
/// use nusa_ipc::RequestId;
///
/// let id = RequestId::new();
/// let id2 = RequestId::new();
/// assert_ne!(id, id2, "RequestIds must be unique");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequestId(pub Uuid);

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// IPC Message Types (M2: IPC Contract v1)
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IpcMessage {
    /// Handshake: Worker announces capabilities
    Hello {
        version: String,
        pid: u32,
        capabilities: Vec<String>,
    },
    /// Acknowledgement from Orchestrator
    Ack,
    /// HTTP Request from Orchestrator to Worker
    Request {
        id: RequestId,
        method: String,
        uri: String,
        headers: HashMap<String, Vec<String>>,
        query: HashMap<String, Vec<String>>,
        post: HashMap<String, Vec<String>>,
        cookies: HashMap<String, String>,
        files: Vec<FileUpload>,
        body: Option<Vec<u8>>,
        server: HashMap<String, String>,
        timeout_ms: u64,
        // A2: Distributed tracing context
        trace_context: Option<TraceContext>,
    },
    /// Response from Worker to Orchestrator
    Response {
        id: RequestId,
        status: u16,
        headers: HashMap<String, Vec<String>>,
        #[serde(with = "json_body")]
        body: Vec<u8>,
        terminated: bool,
    },
    /// Keepalive heartbeat (C1)
    Keepalive {
        timestamp: u64,
    },
    /// Control Signals
    Ping,
    Pong,
    Shutdown,
    Recycle,
    Cancel {
        id: RequestId,
    },
    /// Broadcast event for WebSocket/SSE distribution (F3)
    BroadcastEvent {
        channel: String,
        event: String,
        data: String,
        tenants: Vec<String>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileUpload {
    pub name: String,
    pub filename: String,
    pub mime_type: String,
    pub size: u64,
    pub tmp_path: String,
}

impl IpcMessage {
    /// Serialize to bytes with length prefix (4-byte LE).
    ///
    /// # Example
    /// ```rust
    /// use nusa_ipc::IpcMessage;
    ///
    /// let msg = IpcMessage::Ping;
    /// let bytes = msg.to_framed_bytes().unwrap();
    /// assert!(bytes.len() > 4, "Frame must include 4-byte length prefix");
    /// ```
    pub fn to_framed_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        let payload = serde_json::to_vec(self)?;
        let len = payload.len() as u32;
        let mut frame = len.to_le_bytes().to_vec();
        frame.extend_from_slice(&payload);
        Ok(frame)
    }

    /// Deserialize from framed bytes
    pub fn from_framed_bytes(data: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        if data.len() < 4 {
            return Err("Incomplete frame header".into());
        }
        let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if data.len() < 4 + len {
            return Err("Incomplete payload".into());
        }
        let payload = &data[4..4 + len];
        Ok(serde_json::from_slice(payload)?)
    }

    /// Create a keepalive message with current timestamp.
    #[must_use]
    pub fn keepalive() -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self::Keepalive { timestamp }
    }

    /// Create a broadcast event message.
    #[must_use]
    pub fn broadcast_event(
        channel: String,
        event: String,
        data: String,
        tenants: Vec<String>,
    ) -> Self {
        Self::BroadcastEvent {
            channel,
            event,
            data,
            tenants,
        }
    }
}
