//! UnixSocket transport for IPC communication.
//!
//! Skills applied:
//! - `m07-concurrency`: Async read/write with tokio, background heartbeat
//! - `m06-error-handling`: Framing errors propagate as Results

#[cfg(unix)]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(unix)]
use tokio::net::UnixStream;

use crate::protocol::{IpcMessage, RequestId};
use crate::trace::TraceContext;
use std::collections::HashMap;

/// IPC Transport over UnixSocket.
///
/// m07-concurrency: Uses async read/write with tokio.
pub struct IpcTransport {
    #[cfg(unix)]
    stream: UnixStream,
    #[cfg(not(unix))]
    _phantom: std::marker::PhantomData<()>,
}

impl IpcTransport {
    /// Connect to a UnixSocket server.
    /// On non-Unix platforms, returns a stub transport.
    pub async fn connect(path: &str) -> anyhow::Result<Self> {
        #[cfg(unix)]
        {
            let stream = UnixStream::connect(path).await?;
            Ok(Self { stream })
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            anyhow::bail!("UnixSocket transport is only available on Unix platforms");
        }
    }

    /// Send an IPC message.
    pub async fn send(&mut self, msg: IpcMessage) -> anyhow::Result<()> {
        #[cfg(unix)]
        {
            let framed = msg.to_framed_bytes()?;
            self.stream.write_all(&framed).await?;
            Ok(())
        }
        #[cfg(not(unix))]
        {
            let _ = msg;
            anyhow::bail!("UnixSocket transport is only available on Unix platforms");
        }
    }

    /// Send a keepalive heartbeat (C3).
    pub async fn send_keepalive(&mut self) -> anyhow::Result<()> {
        self.send(IpcMessage::keepalive()).await
    }

    /// Receive an IPC message.
    pub async fn recv(&mut self) -> anyhow::Result<IpcMessage> {
        #[cfg(unix)]
        {
            let mut header = [0u8; 4];
            self.stream.read_exact(&mut header).await?;
            let len = u32::from_le_bytes(header) as usize;

            let mut payload = vec![0u8; len];
            self.stream.read_exact(&mut payload).await?;

            let mut frame_data = Vec::with_capacity(4 + len);
            frame_data.extend_from_slice(&header);
            frame_data.extend_from_slice(&payload);

            IpcMessage::from_framed_bytes(&frame_data)
                .map_err(|e| anyhow::anyhow!("Failed to decode IPC message: {}", e))
        }
        #[cfg(not(unix))]
        {
            anyhow::bail!("UnixSocket transport is only available on Unix platforms");
        }
    }

    /// Send a request and wait for response.
    pub async fn request_response(
        &mut self,
        method: String,
        uri: String,
        headers: HashMap<String, Vec<String>>,
        timeout_ms: u64,
    ) -> anyhow::Result<IpcMessage> {
        let id = RequestId::new();
        let trace_context = TraceContext::from_raw_headers(&headers);

        let request = IpcMessage::Request {
            id,
            method,
            uri,
            headers,
            query: Default::default(),
            post: Default::default(),
            cookies: Default::default(),
            files: vec![],
            body: None,
            server: Default::default(),
            timeout_ms,
            trace_context: Some(trace_context),
        };

        self.send(request).await?;
        let response =
            tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), self.recv())
                .await??;

        Ok(response)
    }

    /// Run a background heartbeat loop.
    /// Returns when the channel is closed or an error occurs.
    #[cfg(unix)]
    pub async fn run_heartbeat(
        &mut self,
        interval_secs: u64,
        missed_heartbeats: &mut u32,
    ) -> anyhow::Result<()> {
        let interval = std::time::Duration::from_secs(interval_secs);
        loop {
            tokio::time::sleep(interval).await;
            self.send(IpcMessage::Ping).await?;

            // Try to read a pong within 5 seconds
            match tokio::time::timeout(std::time::Duration::from_secs(5), self.recv()).await {
                Ok(Ok(IpcMessage::Pong)) => {
                    *missed_heartbeats = 0;
                }
                _ => {
                    *missed_heartbeats += 1;
                    if *missed_heartbeats >= 3 {
                        return Err(anyhow::anyhow!("Worker missed 3 consecutive heartbeats"));
                    }
                }
            }
        }
    }
}
