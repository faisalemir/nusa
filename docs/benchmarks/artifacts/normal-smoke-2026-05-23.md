# Normal mode wrk smoke (2026-05-23)

Git: `2f905c4` | Environment: Alpine musl (Podman) | Profile: **release** | Duration: 15s | Connections: 50 | Threads: 4

| Scenario | URL | P50 | P99 | RPS |
|----------|-----|-----|-----|-----|
| S1 child GET / | `nusa-bench-child` :18080 | 208.71ms | 727.02ms | 230.52 |
| S2 Octane GET /nusa-ping | `nusa-bench-octane` :18081 | 124.50ms | 764.77ms | 446.55 |

Latency columns are wrk thread **average** and **max** (see [normal-mode-report.md](../normal-mode-report.md)).

FPM baseline still required on the same host before GA — see [normal-mode-report.md](../normal-mode-report.md).

Raw: `wrk-child-root-20260523.txt`, `wrk-octane-ping-20260523.txt`.
