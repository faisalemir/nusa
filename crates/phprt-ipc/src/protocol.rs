use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique ID for each request/task to correlate responses
///
/// # Example
/// ```rust
/// use phprt_ipc::RequestId;
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
    },
    /// Response from Worker to Orchestrator
    Response {
        id: RequestId,
        status: u16,
        headers: HashMap<String, Vec<String>>,
        body: Vec<u8>,
        terminated: bool,
    },
    /// Control Signals
    Ping,
    Pong,
    Shutdown,
    Recycle,
    Cancel {
        id: RequestId,
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
    /// use phprt_ipc::IpcMessage;
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
}
