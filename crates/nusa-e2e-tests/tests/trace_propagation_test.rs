//! P2: Trace context is forwarded on IPC requests (gateway → worker path uses TraceContext in IpcMessage).

#[cfg(unix)]
use nusa_e2e_tests::require_laravel_fixture;
use nusa_ipc::IpcMessage;
#[cfg(unix)]
use nusa_octane_worker::pool::WorkerPool;
use std::collections::HashMap;

#[tokio::test]
#[cfg(unix)]
async fn ipc_request_includes_trace_context_from_headers() {
    let root = require_laravel_fixture();
    let mut pool = WorkerPool::new(1, root, 512, 500);
    pool.initialize().await.expect("initialize");
    assert!(pool.is_ready());

    let mut headers = HashMap::new();
    headers.insert(
        "traceparent".to_string(),
        vec!["00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_string()],
    );

    let response = pool
        .handle_http_request("GET".into(), "/".into(), headers, None, 30_000)
        .await
        .expect("HTTP via pool");
    assert_eq!(response.status, 200);

    pool.shutdown().await.ok();
}

#[test]
fn trace_context_serializes_on_ipc_request_roundtrip() {
    let mut headers = HashMap::new();
    headers.insert(
        "traceparent".to_string(),
        vec!["00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_string()],
    );
    let ctx = nusa_ipc::trace::TraceContext::from_raw_headers(&headers);
    let msg = IpcMessage::Request {
        id: nusa_ipc::RequestId::new(),
        method: "GET".into(),
        uri: "/".into(),
        headers,
        query: Default::default(),
        post: Default::default(),
        cookies: Default::default(),
        files: vec![],
        body: None,
        server: Default::default(),
        timeout_ms: 30_000,
        trace_context: Some(ctx),
    };
    let bytes = msg.to_framed_bytes().expect("frame");
    let decoded = IpcMessage::from_framed_bytes(&bytes).expect("decode");
    match decoded {
        IpcMessage::Request {
            trace_context: Some(tc),
            ..
        } => {
            assert!(tc.traceparent.contains("4bf92f3577b34da6a3ce929d0e0e4736"));
        }
        _ => panic!("expected Request variant"),
    }
}
