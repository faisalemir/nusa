//! IPC Transport with UnixSocket (Unix) and TCP (Windows) fallback.
//!
//! Skills applied:
//! - `m07-concurrency`: Async read/write with tokio, background heartbeat
//! - `m06-error-handling`: Framing errors propagate as Results
//! - `m11-ecosystem`: TCP fallback for cross-platform compatibility

#[cfg(unix)]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(not(unix))]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(unix)]
use tokio::net::TcpStream;
#[cfg(not(unix))]
use tokio::net::TcpStream;
#[cfg(unix)]
use tokio::net::UnixStream;

use crate::error::IpcError;
use crate::protocol::{IpcMessage, RequestId};
use crate::trace::TraceContext;
use std::collections::HashMap;

/// IPC Transport (UnixSocket on Unix, TCP on Windows).
///
/// m07-concurrency: Uses async read/write with tokio.
/// m11-ecosystem: TCP fallback for cross-platform compatibility.
pub struct IpcTransport {
    #[cfg(unix)]
    stream: TransportStream,
    #[cfg(not(unix))]
    stream: TcpStream,
}

/// Internal stream variant for Unix platforms.
#[cfg(unix)]
enum TransportStream {
    Unix(UnixStream),
    Tcp(TcpStream),
}

#[cfg(unix)]
impl TransportStream {
    async fn read_exact(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        match self {
            TransportStream::Unix(s) => {
                s.read_exact(buf).await?;
                Ok(())
            }
            TransportStream::Tcp(s) => {
                s.read_exact(buf).await?;
                Ok(())
            }
        }
    }

    async fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        match self {
            TransportStream::Unix(s) => s.write_all(buf).await,
            TransportStream::Tcp(s) => s.write_all(buf).await,
        }
    }
}

impl IpcTransport {
    /// Connect to a UnixSocket or TCP server.
    /// On Unix: tries Unix socket first, falls back to TCP if path contains `:`.
    /// On Windows: uses TCP transport.
    pub async fn connect(path: &str) -> Result<Self, IpcError> {
        #[cfg(unix)]
        {
            // If path looks like a TCP address (contains :), use TCP
            if path.contains(':') && !path.starts_with('/') {
                let stream = TcpStream::connect(path).await?;
                return Ok(Self {
                    stream: TransportStream::Tcp(stream),
                });
            }
            let stream = UnixStream::connect(path).await?;
            Ok(Self {
                stream: TransportStream::Unix(stream),
            })
        }
        #[cfg(not(unix))]
        {
            // On Windows, always use TCP. Path should be host:port.
            let stream = TcpStream::connect(path).await?;
            Ok(Self { stream })
        }
    }

    /// Create a TCP transport directly (useful for Windows or explicit TCP).
    pub async fn connect_tcp(host: &str, port: u16) -> Result<Self, IpcError> {
        let addr = format!("{host}:{port}");
        #[cfg(unix)]
        {
            let stream = TcpStream::connect(&addr).await?;
            Ok(Self {
                stream: TransportStream::Tcp(stream),
            })
        }
        #[cfg(not(unix))]
        {
            let stream = TcpStream::connect(&addr).await?;
            Ok(Self { stream })
        }
    }

    /// Send an IPC message.
    pub async fn send(&mut self, msg: IpcMessage) -> Result<(), IpcError> {
        #[cfg(unix)]
        {
            let framed = msg.to_framed_bytes().map_err(IpcError::Serialization)?;
            self.stream.write_all(&framed).await?;
            Ok(())
        }
        #[cfg(not(unix))]
        {
            let framed = msg.to_framed_bytes().map_err(IpcError::Serialization)?;
            self.stream.write_all(&framed).await?;
            Ok(())
        }
    }

    /// Send a keepalive heartbeat (C3).
    pub async fn send_keepalive(&mut self) -> Result<(), IpcError> {
        self.send(IpcMessage::keepalive()).await
    }

    /// Receive an IPC message.
    pub async fn recv(&mut self) -> Result<IpcMessage, IpcError> {
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

            IpcMessage::from_framed_bytes(&frame_data).map_err(|e| IpcError::Framing(e.to_string()))
        }
        #[cfg(not(unix))]
        {
            let mut header = [0u8; 4];
            self.stream.read_exact(&mut header).await?;
            let len = u32::from_le_bytes(header) as usize;

            let mut payload = vec![0u8; len];
            self.stream.read_exact(&mut payload).await?;

            let mut frame_data = Vec::with_capacity(4 + len);
            frame_data.extend_from_slice(&header);
            frame_data.extend_from_slice(&payload);

            IpcMessage::from_framed_bytes(&frame_data).map_err(|e| IpcError::Framing(e.to_string()))
        }
    }

    /// Send a request and wait for response.
    pub async fn request_response(
        &mut self,
        method: String,
        uri: String,
        headers: HashMap<String, Vec<String>>,
        timeout_ms: u64,
    ) -> Result<IpcMessage, IpcError> {
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
                .await
                .map_err(|_| IpcError::Handshake("timeout waiting for response".into()))??;

        Ok(response)
    }

    /// Run a background heartbeat loop.
    /// Returns when the channel is closed or an error occurs.
    pub async fn run_heartbeat(
        &mut self,
        interval_secs: u64,
        missed_heartbeats: &mut u32,
    ) -> Result<(), IpcError> {
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
                        return Err(IpcError::MissedHeartbeats(*missed_heartbeats));
                    }
                }
            }
        }
    }
}
