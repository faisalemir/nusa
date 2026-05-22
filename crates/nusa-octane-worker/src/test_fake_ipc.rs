//! In-process fake Octane IPC worker for gateway/pool tests.
//!
//! STUB_CONTRACT: not PHP; proves HTTP dispatch via `WorkerPool::handle_http_request` only.

use std::collections::HashMap;

use nusa_ipc::protocol::IpcMessage;
use nusa_ipc::transport::IpcTransport;
use tokio::net::TcpListener;

use crate::error::WorkerError;
use crate::pool::Worker;

async fn serve_fake_worker(mut server: IpcTransport) {
    loop {
        let msg = match server.recv().await {
            Ok(m) => m,
            Err(_) => break,
        };
        let reply = match msg {
            IpcMessage::Hello { .. } => IpcMessage::Ack,
            IpcMessage::Request {
                id,
                method,
                uri,
                body,
                ..
            } => {
                let mut payload = format!("octane:{method}:{uri}").into_bytes();
                if let Some(b) = body {
                    payload.extend_from_slice(b":");
                    payload.extend_from_slice(&b);
                }
                IpcMessage::Response {
                    id,
                    status: 200,
                    headers: HashMap::new(),
                    body: payload,
                    terminated: true,
                }
            }
            IpcMessage::Ping => IpcMessage::Pong,
            _ => continue,
        };
        if server.send(reply).await.is_err() {
            break;
        }
    }
}

/// Spawn a worker connected to a loopback fake IPC server (no PHP).
#[doc(hidden)]
pub async fn spawn_fake_tcp_worker(id: usize) -> Result<Worker, WorkerError> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| WorkerError::Handshake(format!("fake worker bind: {e}")))?;
    let addr = listener
        .local_addr()
        .map_err(|e| WorkerError::Handshake(format!("fake worker addr: {e}")))?;
    let addr_str = format!("127.0.0.1:{}", addr.port());

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let server = IpcTransport::from_connected_tcp(stream);
            tokio::spawn(serve_fake_worker(server));
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let mut transport = IpcTransport::connect(&addr_str)
        .await
        .map_err(|e| WorkerError::Handshake(format!("fake worker connect: {e}")))?;

    let hello = IpcMessage::Hello {
        version: "1.0".to_string(),
        pid: std::process::id(),
        capabilities: vec!["http".to_string()],
    };
    transport
        .send(hello)
        .await
        .map_err(|e| WorkerError::Handshake(e.to_string()))?;

    match tokio::time::timeout(std::time::Duration::from_secs(2), transport.recv()).await {
        Ok(Ok(IpcMessage::Ack)) => {}
        Ok(Ok(other)) => {
            return Err(WorkerError::Handshake(format!(
                "fake worker expected Ack, got {other:?}"
            )));
        }
        Ok(Err(e)) => return Err(WorkerError::Handshake(e.to_string())),
        Err(_) => {
            return Err(WorkerError::Handshake(
                "fake worker handshake timeout".into(),
            ));
        }
    }

    Ok(Worker::new_test_with_transport(id, transport))
}
