# Octane IPC latency report (M2)

**Benchmark:** `benches/src/ipc_latency_bench.rs` (`nusa-benchmarks` package).

## Gate (P2)

| Metric | Target | Command |
|--------|--------|---------|
| P99 round-trip | ≤ 10 ms | `just bench-fast ipc_latency_bench` (release) |
| Alpine smoke | No regression vs host | `just podman-bench-ipc-smoke` |

Criterion prints percentiles to stdout. The harness measures **in-process framed serialize/deserialize** (not a live PHP worker). End-to-end Laravel IPC: `just podman-test-laravel`.

## Methodology

- Framed JSON `IpcMessage::Request` / `Response` encode + decode in a tight loop.
- Release profile (`cargo bench` default).
- Alpine: `just podman-bench-ipc-smoke` inside `nusa-test-runner` image.

## Results

| Date | Git SHA | Scenario | Median (est.) | P99 (est.) | Pass (≤10 ms P99) | Environment |
|------|---------|----------|---------------|------------|-------------------|-------------|
| 2026-05-23 | `38fc6d0` + working tree | `serialize_deserialize` / `small` | _see Criterion output_ | _see Criterion output_ | Smoke | Alpine `podman-bench-ipc-smoke` |
| 2026-05-23 | `38fc6d0` + working tree | `serialize_deserialize` / `large_body` (100 KiB) | **~6.8 ms** | **~6.9 ms** | **Yes** (framing only) | Alpine `podman-bench-ipc-smoke` |
| 2026-05-23 | `38fc6d0` + working tree | `keepalive` | **~582 ns** | _see Criterion output_ | N/A | Alpine `podman-bench-ipc-smoke` |

**Interpretation:** Large-body framing median ~6.8 ms is within the 10 ms P2 smoke budget for serialization. Do not treat this as PHP worker round-trip latency—use `nusa-e2e-tests` and load tests for that.

### Leak suite (P2)

| Date | Requests | Result | Command |
|------|----------|--------|---------|
| 2026-05-23 | 10,000 | **Pass** | `just podman-test-laravel-leak` (`octane_leak_suite_sequential_requests_stable_body`) |

## Next recording (before v1.0.0 tag)

1. Re-run `just podman-bench-ipc-smoke` on release hardware; paste full Criterion table (P50/P95/P99) for all three scenarios.
2. Add wrk/k6 Octane route latency beside framing numbers.
3. Link commit SHA after tag from [`docs/public/production-status.md`](../public/production-status.md).
