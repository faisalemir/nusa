# Benchmarks

Performance evidence for Nusa GA (M1 Normal Mode, M2 IPC). All release numbers must be produced on **Alpine musl** unless noted.

## Commands

| Benchmark | Command |
|-----------|---------|
| Full suite | `just bench` |
| IPC latency (P2 KPI) | `just bench-fast ipc_latency_bench` |
| Alpine IPC smoke | `just podman-bench-ipc-smoke` |

**P2 target:** IPC round-trip P99 ≤ 10 ms (`ipc_latency_bench`, release profile).

## Reports

| Document | Scope |
|----------|--------|
| [normal-mode-report.md](normal-mode-report.md) | Child engine vs FPM baseline (M1 pilot) |
| [octane-ipc-report.md](octane-ipc-report.md) | IPC latency methodology and gate |

## Load scripts

See [`tests/load/README.md`](../../tests/load/README.md) for k6/wrk scenarios used in P3.
