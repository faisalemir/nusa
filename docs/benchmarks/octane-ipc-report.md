# Octane IPC latency report (M2)

**Benchmark:** `benches/src/ipc_latency_bench.rs` (`nusa-benchmarks` package).

## Gate (P2)

| Metric | Target | Command |
|--------|--------|---------|
| P99 round-trip | ≤ 10 ms | `just bench-fast ipc_latency_bench` (release) |
| Alpine smoke | No regression vs host | `just podman-bench-ipc-smoke` |

Criterion prints percentiles to stdout; record the `ipc_latency` group P99 after each release candidate.

## Methodology

- Framed JSON `IpcMessage::Request` / `Response` over Unix socket (in-process echo server in bench harness).
- Release profile (`cargo bench` default).
- Not a full PHP worker round-trip — use `just podman-test-laravel` for end-to-end Laravel IPC.

## Results template

| Date | Git SHA | P50 (µs) | P99 (µs) | Pass (≤10ms P99) |
|------|---------|----------|----------|------------------|
| _TBD_ | _TBD_ | _TBD_ | _TBD_ | _TBD_ |
