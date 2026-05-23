# Normal Mode benchmark report (M1 pilot)

**Status:** Pilot methodology documented — wrk/k6 numbers still **TBD** before GA tag.  
**Environment:** Alpine Linux musl (`just podman-ci`), `engine = child`, `octane_workers = 0`.  
**CI (2026-05-23):** `just podman-ci-fast` / `just podman-ci` green on working tree; soak tests in workspace (e.g. `gateway_soak_30_seconds_memory_stable`) pass in Alpine.

## Scenarios

| ID | Workload | Tool | Notes |
|----|----------|------|-------|
| S1 | Static `index.php` echo | wrk / k6 | Baseline PHP overhead |
| S2 | Laravel route (fixture) | k6 | After P2 fixture available |
| S3 | Concurrent tenants | k6 | `X-Tenant-Id` rotation |

Scripts: [`tests/load/README.md`](../../tests/load/README.md).

## KPI (blueprint M1)

| Metric | Target | How to verify |
|--------|--------|---------------|
| P50 latency | ≤ PHP-FPM P50 (same hardware) | Compare wrk reports |
| P99 latency | ≤ FPM P99 + 30% | Same run, same concurrency |
| RSS drift | Stable over 30 min soak | `nusa-gateway` soak tests + host RSS |

## Recording results

1. Run baseline FPM/nginx on the same VM as Nusa.
2. Run Nusa with `config.toml.example` (child engine).
3. Capture P50/P99 from wrk: `wrk -t4 -c100 -d30s http://127.0.0.1:8080/`
4. Paste tables below and commit before v1.0.0 tag.

### Results (fill on Alpine)

| Scenario | Engine | P50 (ms) | P99 (ms) | RPS |
|----------|--------|----------|----------|-----|
| S1 | FPM | _TBD_ | _TBD_ | _TBD_ |
| S1 | Nusa child | _TBD_ | _TBD_ | _TBD_ |
| S2 | Nusa child | _TBD_ | _TBD_ | _TBD_ |

## Child engine E2E

Integration coverage: `tests/integration/engine_integration_test.rs` (mock engine lifecycle).  
Live PHP child paths are validated in Alpine via gateway + config tests; extend with `public/index.php` fixture when added.

## Sign-off

- [x] Alpine musl workspace + gateway soak tests (`just podman-ci-fast`)  
- [ ] wrk/k6 P50/P99 compared against FPM on same host  
- [x] Linked from [`docs/public/production-status.md`](../public/production-status.md)
