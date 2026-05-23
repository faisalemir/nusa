# Normal Mode benchmark report (M1 pilot)

**Status:** Alpine wrk smoke recorded (2026-05-23, commit `2f905c4`) — **PHP-FPM baseline on the same host** still required before **v1.0.0** tag.  
**Environment:** Alpine Linux musl (Podman `nusa-test-runner`), `engine = child`, release build (`NUSA_BENCH_RELEASE=1`).  
**CI:** `just podman-ci` / `just podman-ci-e2e` include `just podman-bench-normal-smoke` (default 5s / 10 conn smoke).

## Methodology

| Parameter | GA smoke run (2026-05-23) | Default CI smoke |
|-----------|---------------------------|------------------|
| Tool | wrk 4.x | wrk |
| Threads / connections | 4 / 50 | 2 / 10 |
| Duration | 15s | 5s |
| Profile | `cargo build --release -p nusa-cli` | debug |
| Latency columns | wrk **Thread Stats** avg → “P50”, max → “P99” (not HdrHistogram) | same |

Configs: [`tests/load/nusa-bench-child.toml`](../../tests/load/nusa-bench-child.toml), [`tests/load/nusa-bench-octane.toml`](../../tests/load/nusa-bench-octane.toml).  
`max_workers = 128` on bench configs to avoid gateway backpressure (503) under load.

Reproduce GA-style numbers:

```bash
podman run --rm -t -v "$(pwd):/src:Z" -w /src \
  -e NUSA_BENCH_RELEASE=1 -e WRK_DURATION=15s -e WRK_CONNECTIONS=50 -e WRK_THREADS=4 \
  nusa-test-runner sh -c "cp -f .cargo/config-alpine.toml .cargo/config.toml && sh tests/load/run-alpine-bench-smoke.sh"
```

Or: `just podman-bench-normal-smoke` (shorter defaults).

## Scenarios

| ID | Workload | Tool | Notes |
|----|----------|------|-------|
| S1 | Child IPC `GET /` (`php-static-minimal`) | wrk | Normal mode (`octane_workers = 0`) |
| S2 | Octane `GET /nusa-ping` (Laravel minimal fixture) | wrk | `octane_workers = 4` |
| S3 | Concurrent tenants | k6 | `tests/load/laravel-fixture.js` — optional |
| S1-FPM | nginx + php-fpm same route class | wrk | **TBD** — same VM, same wrk flags |

Scripts: [`tests/load/README.md`](../../tests/load/README.md).

## KPI (blueprint M1)

| Metric | Target | Status |
|--------|--------|--------|
| P50 latency | ≤ PHP-FPM P50 (same hardware) | **Blocked** — FPM row empty |
| P99 latency | ≤ FPM P99 + 30% | **Blocked** — FPM row empty |
| RSS drift | Stable over 30 min soak | **Pass** — gateway soak tests in `just podman-ci-fast` |

## Results (Alpine musl, 2026-05-23)

Source artifact: [`artifacts/normal-smoke-2026-05-23.md`](artifacts/normal-smoke-2026-05-23.md), raw wrk: `artifacts/wrk-*-20260523.txt`.

| Scenario | Engine | P50 (ms) | P99 (ms) | RPS | Notes |
|----------|--------|----------|----------|-----|-------|
| S1-FPM | php-fpm + nginx | _TBD_ | _TBD_ | _TBD_ | Same host + wrk flags as below |
| S1 | Nusa child `GET /` | 208.71 | 727.02 | 230.52 | All 2xx in this run |
| S2 | Nusa Octane `/nusa-ping` | 124.50 | 764.77 | 446.55 | 4 workers; warm-up request before wrk |

**Interpretation:** Child mode pays per-request PHP process + IPC cost; Octane amortizes Laravel bootstrap. Compare S1 to S1-FPM only after FPM baseline is recorded on identical hardware.

## Child engine E2E

- Fixture: `tests/fixtures/php-static-minimal/` + `bootstrap/nusa-child-ipc.php`
- Config: `php_bootstrap` in `nusa-bench-child.toml`
- Test: `nusa-engine-child` `child_bootstrap_fixture_test` (requires `php` on PATH; full path in Alpine CI)

## Sign-off

- [x] Alpine musl workspace + gateway soak tests (`just podman-ci-fast`)
- [x] Alpine wrk smoke automated (`just podman-bench-normal-smoke`)
- [x] Documented reproducible release wrk run (table above)
- [ ] wrk/k6 P50/P99 **vs FPM** on same host (GA v1.0.0)
- [x] Linked from [`docs/public/production-status.md`](../public/production-status.md)

## v1.0.0 release

Do **not** tag v1.0.0 until the S1-FPM row is filled and [`docs/contributor/release-checklist.md`](../contributor/release-checklist.md) gates pass. Use `just release-bump major` when ready.
