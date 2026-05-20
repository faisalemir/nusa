//! IPC benchmarks: Serialization, framing, codec
//!
//! Run: cargo bench -p nusa-benchmarks --bench ipc_bench

use criterion::{criterion_group, criterion_main, Criterion, black_box};
use std::time::Duration;

use nusa_ipc::{IpcMessage, RequestId};

fn bench_ipc_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("ipc_serialization");

    let hello = IpcMessage::Hello {
        version: "1.0".to_string(),
        pid: 12345,
        capabilities: vec!["http".into(), "tasks".into()],
    };

    group.bench_function("hello_to_framed_bytes", |b| {
        b.iter(|| black_box(&hello).to_framed_bytes())
    });

    let ping = IpcMessage::Ping;
    group.bench_function("ping_to_framed_bytes", |b| {
        b.iter(|| black_box(&ping).to_framed_bytes())
    });

    let id = RequestId::new();
    let request = IpcMessage::Request {
        id,
        method: "GET".into(),
        uri: "/api/users".into(),
        headers: Default::default(),
        query: Default::default(),
        post: Default::default(),
        cookies: Default::default(),
        files: vec![],
        body: None,
        server: Default::default(),
        timeout_ms: 30_000,
    };
    group.bench_function("request_to_framed_bytes", |b| {
        b.iter(|| black_box(&request).to_framed_bytes())
    });

    group.finish();
}

fn bench_ipc_deserialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("ipc_deserialization");

    let msg = IpcMessage::Hello {
        version: "1.0".to_string(),
        pid: 12345,
        capabilities: vec!["http".into(), "tasks".into()],
    };
    let framed = msg.to_framed_bytes().unwrap();

    group.bench_function("from_framed_bytes_hello", |b| {
        b.iter(|| black_box(IpcMessage::from_framed_bytes(&framed)))
    });

    let ping = IpcMessage::Ping;
    let ping_framed = ping.to_framed_bytes().unwrap();
    group.bench_function("from_framed_bytes_ping", |b| {
        b.iter(|| black_box(IpcMessage::from_framed_bytes(&ping_framed)))
    });

    group.finish();
}

fn bench_request_id(c: &mut Criterion) {
    let mut group = c.benchmark_group("request_id");

    group.bench_function("new", |b| {
        b.iter(|| black_box(RequestId::new()))
    });

    group.bench_function("default", |b| {
        b.iter(|| black_box(RequestId::default()))
    });

    group.finish();
}

criterion_group!(
    name = ipc_benches;
    config = Criterion::default().measurement_time(Duration::from_secs(3));
    targets = bench_ipc_serialization, bench_ipc_deserialization, bench_request_id
);
criterion_main!(ipc_benches);
