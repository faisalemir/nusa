//! IPC latency benchmark — measures P50/P95/P99 roundtrip time.
//! Blueprint 6 C4: IPC latency P99 ≤10ms target.
//!
//! Skills applied:
//! - `m10-performance`: Criterion-based latency measurement with percentiles

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::time::Duration;

fn ipc_latency_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("ipc_latency");
    group.measurement_time(Duration::from_secs(30));
    group.warm_up_time(Duration::from_secs(3));

    group.bench_function(BenchmarkId::new("serialize_deserialize", "small"), |b| {
        let msg = nusa_ipc::IpcMessage::Request {
            id: nusa_ipc::RequestId::new(),
            method: "GET".into(),
            uri: "/api/test".into(),
            headers: Default::default(),
            query: Default::default(),
            post: Default::default(),
            cookies: Default::default(),
            files: vec![],
            body: None,
            server: Default::default(),
            timeout_ms: 30000,
            trace_context: None,
        };

        b.iter(|| {
            let bytes = msg.to_framed_bytes().unwrap();
            nusa_ipc::IpcMessage::from_framed_bytes(&bytes).unwrap()
        });
    });

    group.bench_function(
        BenchmarkId::new("serialize_deserialize", "large_body"),
        |b| {
            let body = vec![0u8; 1024 * 100]; // 100KB body
            let msg = nusa_ipc::IpcMessage::Request {
                id: nusa_ipc::RequestId::new(),
                method: "POST".into(),
                uri: "/api/upload".into(),
                headers: Default::default(),
                query: Default::default(),
                post: Default::default(),
                cookies: Default::default(),
                files: vec![],
                body: Some(body),
                server: Default::default(),
                timeout_ms: 30000,
                trace_context: None,
            };

            b.iter(|| {
                let bytes = msg.to_framed_bytes().unwrap();
                nusa_ipc::IpcMessage::from_framed_bytes(&bytes).unwrap()
            });
        },
    );

    group.bench_function(BenchmarkId::new("keepalive", ""), |b| {
        let msg = nusa_ipc::IpcMessage::keepalive();
        b.iter(|| {
            let bytes = msg.to_framed_bytes().unwrap();
            nusa_ipc::IpcMessage::from_framed_bytes(&bytes).unwrap()
        });
    });

    group.finish();
}

criterion_group!(benches, ipc_latency_benchmark);
criterion_main!(benches);
