//! Long-lived PHP embed daemon over stdin/stdout (length-prefixed JSON).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use bytes::Bytes;
use http::header::{HeaderName, HeaderValue};
use nusa_core::AsyncIoBridge;
use nusa_core::async_io::{AsyncSqlResult, async_io_stub_from_env};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;

use crate::error::EmbedError;
use crate::frame;
use crate::paths::{resolve_embed_daemon, resolve_php_driver_root};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmbedTransport {
    Json,
    Frame,
}

fn transport_from_env() -> EmbedTransport {
    match std::env::var("NUSA_EMBED_TRANSPORT").ok().as_deref() {
        Some("json") => EmbedTransport::Json,
        _ => EmbedTransport::Frame,
    }
}

/// One persistent PHP process running `nusa_embed_daemon.php`.
pub struct StdioEmbedWorker {
    pub id: usize,
    transport: EmbedTransport,
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<ChildStdout>,
    requests_handled: AtomicU64,
    child: Mutex<Child>,
    booted: bool,
    async_io: Arc<dyn AsyncIoBridge>,
}

impl StdioEmbedWorker {
    pub async fn spawn(
        id: usize,
        code_dir: PathBuf,
        php_binary: &str,
        async_io: Arc<dyn AsyncIoBridge>,
    ) -> Result<Self, EmbedError> {
        let daemon = resolve_embed_daemon(&code_dir)?;
        let driver_root = resolve_php_driver_root(&code_dir);
        let transport = transport_from_env();

        let mut cmd = tokio::process::Command::new(php_binary);
        cmd.args(["-d", "output_buffering=0", "-d", "implicit_flush=1"])
            .arg(&daemon)
            .current_dir(&code_dir)
            .env("NUSA_CODE_DIR", code_dir.as_os_str());
        if let Some(root) = driver_root {
            cmd.env("NUSA_PHP_DRIVER", root.as_os_str());
        }
        if transport == EmbedTransport::Frame {
            cmd.env("NUSA_EMBED_TRANSPORT", "frame");
        }
        if async_io_stub_from_env() {
            cmd.env("NUSA_ASYNC_IO", "stub");
        }

        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| EmbedError::handshake(format!("spawn php: {e}")))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| EmbedError::handshake("stdin not piped"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| EmbedError::handshake("stdout not piped"))?;

        let mut worker = Self {
            id,
            transport,
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(stdout),
            requests_handled: AtomicU64::new(0),
            child: Mutex::new(child),
            booted: false,
            async_io,
        };

        worker.bootstrap(&code_dir).await?;
        worker.booted = true;
        Ok(worker)
    }

    pub fn is_booted(&self) -> bool {
        self.booted
    }

    pub fn requests_handled(&self) -> u64 {
        self.requests_handled.load(Ordering::SeqCst)
    }

    async fn bootstrap(&mut self, code_dir: &Path) -> Result<(), EmbedError> {
        match self.transport {
            EmbedTransport::Json => {
                let msg = serde_json::json!({
                    "type": "Bootstrap",
                    "code_dir": code_dir.to_string_lossy(),
                });
                self.write_message(&msg).await?;
                let reply = self.read_message().await?;
                if reply.get("type").and_then(|v| v.as_str()) != Some("Ack") {
                    return Err(EmbedError::handshake(format!("expected Ack, got {reply}")));
                }
            }
            EmbedTransport::Frame => {
                let frame = frame::encode_bootstrap(&code_dir.to_string_lossy())?;
                self.write_bytes(&frame).await?;
                let reply = self.read_bytes().await?;
                if let Ok(msg) = frame::decode_error(&reply) {
                    return Err(EmbedError::handshake(msg));
                }
                frame::decode_ack(&reply)?;
            }
        }
        Ok(())
    }

    pub async fn handle_request(
        &self,
        method: String,
        uri: String,
        headers: HashMap<String, Vec<String>>,
        body: Option<Vec<u8>>,
    ) -> Result<nusa_core::PhpResponse, EmbedError> {
        let body_bytes = body.unwrap_or_default();

        let (status, header_map, response_body) = match self.transport {
            EmbedTransport::Json => {
                self.handle_request_json(method, uri, headers, body_bytes)
                    .await?
            }
            EmbedTransport::Frame => {
                self.handle_request_frame(method, uri, headers, body_bytes)
                    .await?
            }
        };

        self.requests_handled.fetch_add(1, Ordering::SeqCst);

        Ok(nusa_core::PhpResponse {
            status,
            headers: header_map,
            body: response_body,
        })
    }

    async fn handle_request_json(
        &self,
        method: String,
        uri: String,
        headers: HashMap<String, Vec<String>>,
        body_bytes: Vec<u8>,
    ) -> Result<(u16, http::HeaderMap, Bytes), EmbedError> {
        let body_field: serde_json::Value = if body_bytes.is_empty() {
            serde_json::Value::String(String::new())
        } else {
            serde_json::Value::Array(
                body_bytes
                    .into_iter()
                    .map(serde_json::Value::from)
                    .collect(),
            )
        };

        let msg = serde_json::json!({
            "type": "Request",
            "method": method,
            "uri": uri,
            "headers": headers,
            "body": body_field,
            "cookies": {},
        });

        self.write_message(&msg).await?;
        let reply = self.read_message().await?;
        self.php_response_from_json(reply)
    }

    async fn handle_request_frame(
        &self,
        method: String,
        uri: String,
        headers: HashMap<String, Vec<String>>,
        body_bytes: Vec<u8>,
    ) -> Result<(u16, http::HeaderMap, Bytes), EmbedError> {
        let frame = frame::encode_request(&method, &uri, &headers, &body_bytes)?;
        self.write_bytes(&frame).await?;
        self.read_until_http_response().await
    }

    /// Multiplexed read: answer inline `OP_ASYNC_QUERY` until `OP_RESPONSE`.
    async fn read_until_http_response(&self) -> Result<(u16, http::HeaderMap, Bytes), EmbedError> {
        loop {
            let reply = self.read_bytes().await?;
            if let Ok(msg) = frame::decode_error(&reply) {
                return Err(EmbedError::Worker(self.id, msg));
            }
            let op = frame::frame_op(&reply)?;
            match op {
                frame::OP_RESPONSE => {
                    let decoded = frame::decode_response(&reply)?;
                    return Ok((
                        decoded.status,
                        Self::ipc_headers_to_http(&decoded.headers),
                        decoded.body,
                    ));
                }
                frame::OP_ASYNC_QUERY => {
                    let sql = frame::decode_async_query(&reply)?;
                    let result = self.dispatch_async_query(&sql).await?;
                    self.write_bytes(&result).await?;
                }
                other => {
                    return Err(EmbedError::Json(format!(
                        "unexpected frame op {other} while awaiting HTTP response"
                    )));
                }
            }
        }
    }

    async fn dispatch_async_query(&self, sql: &str) -> Result<Vec<u8>, EmbedError> {
        if !self.async_io.enabled().await {
            return frame::encode_async_result_err("async I/O bridge disabled");
        }
        match self.async_io.execute_readonly_sql(sql).await {
            Ok(AsyncSqlResult::Scalar(value)) => frame::encode_async_result_ok(value),
            Ok(AsyncSqlResult::Rows(rows)) => {
                let json =
                    serde_json::to_string(&rows).map_err(|e| EmbedError::Json(e.to_string()))?;
                frame::encode_async_result_json(&json)
            }
            Err(err) => frame::encode_async_result_err(&err.to_string()),
        }
    }

    fn php_response_from_json(
        &self,
        reply: serde_json::Value,
    ) -> Result<(u16, http::HeaderMap, Bytes), EmbedError> {
        if reply.get("type").and_then(|v| v.as_str()) == Some("Error") {
            return Err(EmbedError::Worker(
                self.id,
                reply
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("embed error")
                    .to_string(),
            ));
        }

        let status = reply.get("status").and_then(|v| v.as_u64()).unwrap_or(500) as u16;
        let body_str = reply
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let mut header_map = http::HeaderMap::new();
        if let Some(hdrs) = reply.get("headers").and_then(|v| v.as_object()) {
            for (name, values) in hdrs {
                let Ok(header_name) = HeaderName::from_bytes(name.as_bytes()) else {
                    continue;
                };
                let arr = values
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_else(|| {
                        values
                            .as_str()
                            .map(|s| vec![s.to_string()])
                            .unwrap_or_default()
                    });
                for value in arr {
                    if let Ok(header_value) = HeaderValue::from_str(&value) {
                        header_map.append(header_name.clone(), header_value);
                    }
                }
            }
        }

        Ok((status, header_map, Bytes::from(body_str)))
    }

    fn ipc_headers_to_http(headers: &HashMap<String, Vec<String>>) -> http::HeaderMap {
        let mut map = http::HeaderMap::new();
        for (name, values) in headers {
            let Ok(header_name) = HeaderName::from_bytes(name.as_bytes()) else {
                continue;
            };
            for value in values {
                if let Ok(header_value) = HeaderValue::from_str(value) {
                    map.append(header_name.clone(), header_value);
                }
            }
        }
        map
    }

    pub async fn shutdown(&mut self) {
        let mut child = self.child.lock().await;
        let _ = child.kill().await;
        let _ = child.wait().await;
        self.booted = false;
    }

    async fn write_bytes(&self, frame: &[u8]) -> Result<(), EmbedError> {
        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(frame)
            .await
            .map_err(|e| EmbedError::Io(e.to_string()))?;
        stdin
            .flush()
            .await
            .map_err(|e| EmbedError::Io(e.to_string()))?;
        Ok(())
    }

    async fn read_bytes(&self) -> Result<Vec<u8>, EmbedError> {
        let mut stdout = self.stdout.lock().await;
        let mut header = [0u8; 4];
        stdout
            .read_exact(&mut header)
            .await
            .map_err(|e| EmbedError::Io(e.to_string()))?;
        let len = u32::from_le_bytes(header) as usize;
        let mut payload = vec![0u8; len];
        stdout
            .read_exact(&mut payload)
            .await
            .map_err(|e| EmbedError::Io(e.to_string()))?;
        // Prepend the 4-byte length header into payload in-place.
        // This avoids a third Vec allocation.
        payload.splice(0..0, header);
        Ok(payload)
    }

    async fn write_message(&self, value: &serde_json::Value) -> Result<(), EmbedError> {
        let json = serde_json::to_vec(value).map_err(|e| EmbedError::Json(e.to_string()))?;
        let mut frame = (json.len() as u32).to_le_bytes().to_vec();
        frame.extend_from_slice(&json);
        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(&frame)
            .await
            .map_err(|e| EmbedError::Io(e.to_string()))?;
        stdin
            .flush()
            .await
            .map_err(|e| EmbedError::Io(e.to_string()))?;
        Ok(())
    }

    async fn read_message(&self) -> Result<serde_json::Value, EmbedError> {
        let mut stdout = self.stdout.lock().await;
        let mut header = [0u8; 4];
        stdout
            .read_exact(&mut header)
            .await
            .map_err(|e| EmbedError::Io(e.to_string()))?;
        let len = u32::from_le_bytes(header) as usize;
        let mut payload = vec![0u8; len];
        stdout
            .read_exact(&mut payload)
            .await
            .map_err(|e| EmbedError::Io(e.to_string()))?;
        serde_json::from_slice(&payload).map_err(|e| EmbedError::Json(e.to_string()))
    }
}
