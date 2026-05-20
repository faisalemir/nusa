//! Gateway benchmarks: Circuit breaker, health state, resource guards
//!
//! Run: `cargo bench -p nusa-benchmarks --bench gateway_bench`

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::time::Duration;

use nusa_core::{validate_request_size, BackpressureGuard, ResourceGuard};
use nusa_gateway::circuit_breaker::CircuitBreaker;
use nusa_gateway::health::HealthState;

fn bench_circuit_breaker(c: &mut Criterion) {
    let mut group = c.benchmark_group("circuit_breaker");

    group.bench_function("allow_request_closed", |b| {
        let cb = CircuitBreaker::new(3, Duration::from_secs(1));
        b.iter(|| black_box(cb.allow_request()))
    });

    group.bench_function("allow_request_opened", |b| {
        let cb = CircuitBreaker::new(1, Duration::from_secs(10));
        cb.record_failure();
        b.iter(|| black_box(cb.allow_request()))
    });

    group.bench_function("record_failure", |b| {
        let cb = CircuitBreaker::new(100, Duration::from_secs(1));
        b.iter(|| black_box(cb.record_failure()))
    });

    group.finish();
}

fn bench_health_state(c: &mut Criterion) {
    let mut group = c.benchmark_group("health_state");

    group.bench_function("record_success", |b| {
        let state = HealthState::new();
        b.iter(|| black_box(state.record_success()))
    });

    group.bench_function("record_error", |b| {
        let state = HealthState::new();
        b.iter(|| black_box(state.record_error()))
    });

    group.bench_function("is_ready", |b| {
        let state = HealthState::new();
        state.mark_ready();
        b.iter(|| black_box(state.is_ready()))
    });

    group.finish();
}

fn bench_resource_guards(c: &mut Criterion) {
    let mut group = c.benchmark_group("resource_guards");

    group.bench_function("validate_request_size_within", |b| {
        b.iter(|| black_box(validate_request_size(Some(500), 1024)))
    });

    group.bench_function("validate_request_size_exceeds", |b| {
        b.iter(|| black_box(validate_request_size(Some(2000), 1024)))
    });

    group.bench_function("validate_request_size_none", |b| {
        b.iter(|| black_box(validate_request_size(None, 1024)))
    });

    group.finish();
}

criterion_group!(benches, bench_circuit_breaker, bench_health_state, bench_resource_guards);
criterion_main!(benches);
