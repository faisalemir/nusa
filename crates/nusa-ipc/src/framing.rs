//! IPC framing codec for tokio-util.
//!
//! Skills applied:
//! - `m06-error-handling`: IO errors propagate properly
//! - `m11-ecosystem`: tokio-util codec integration

use bytes::BytesMut;
use tokio_util::codec::{Decoder, Encoder};

use crate::protocol::IpcMessage;

/// Maximum frame payload size (64 MB)
const DEFAULT_MAX_FRAME_SIZE: usize = 64 * 1024 * 1024;

/// Length-prefixed framing codec for tokio-util.
/// Wire format: [4 bytes length LE] [JSON payload...]
pub struct IpcCodec {
    max_frame_size: usize,
}

impl IpcCodec {
    pub fn new() -> Self {
        Self {
            max_frame_size: DEFAULT_MAX_FRAME_SIZE,
        }
    }

    pub fn with_max_frame_size(max_frame_size: usize) -> Self {
        Self { max_frame_size }
    }
}

impl Default for IpcCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder for IpcCodec {
    type Item = IpcMessage;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < 4 {
            return Ok(None);
        }

        let len = u32::from_le_bytes([src[0], src[1], src[2], src[3]]) as usize;

        if len > self.max_frame_size {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Frame too large: {len} bytes"),
            ));
        }

        let total_needed = 4 + len;
        if src.len() < total_needed {
            src.reserve(total_needed - src.len());
            return Ok(None);
        }

        let payload_bytes = src.split_to(total_needed).freeze().slice(4..total_needed);
        let msg = serde_json::from_slice(&payload_bytes).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("JSON parse error: {e}"),
            )
        })?;

        Ok(Some(msg))
    }
}

impl Encoder<IpcMessage> for IpcCodec {
    type Error = std::io::Error;

    fn encode(&mut self, item: IpcMessage, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let framed = item.to_framed_bytes().map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Serialize error: {e}"),
            )
        })?;
        dst.extend_from_slice(&framed);
        Ok(())
    }
}
