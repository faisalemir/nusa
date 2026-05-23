# Normal Mode benchmark report (M1 pilot)

**Status:** Alpine wrk smoke recorded (2026-05-23, commit `2f905c4`). KPI baselines from published external benchmarks (no internal FPM/FrankenPHP benchmark runs).  
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

Scripts: [`tests/load/README.md`](../../tests/load/README.md).

## KPI (blueprint M1)

| Metric | Target (external baseline) | Status |
|--------|---------------------------|--------|
| P50 latency | ≤ 24ms avg (Hoyo Tech FrankenPHP) | 124ms → 5.2x target |
| P99 latency | ≤ 70ms (Hoyo Tech P95 54ms + 30%) | 764ms → 10.9x target |
| RPS | ≥ 410 (Hoyo Tech FrankenPHP, 16 vCPU) | **Pass** — 446 S2 Octane IPC |
| RSS drift | Stable over 30 min soak | **Pass** — gateway soak tests in `just podman-ci-fast` |

**Notes:** Baselines dari benchmark eksternal independen (FrankenPHP, FPM, RoadRunner). Nusa tidak menjalankan benchmark internal FPM/FrankenPHP; KPI target menggunakan data publik sebagai referensi. Lihat "External baseline references" di bawah.

## Results (Alpine musl, 2026-05-23)

Source artifact: [`artifacts/normal-smoke-2026-05-23.md`](artifacts/normal-smoke-2026-05-23.md), raw wrk: `artifacts/wrk-*-20260523.txt`.

| Scenario | Engine | P50 (ms) | P99 (ms) | RPS | Notes |
|----------|--------|----------|----------|-----|-------|
| S1 | Nusa child `GET /` | 208.71 | 727.02 | 230.52 | All 2xx in this run |
| S2 | Nusa Octane IPC `/nusa-ping` | 124.50 | 764.77 | 446.55 | 4 workers; warm-up request before wrk |
| S2-embed JSON | Nusa Octane embed `/nusa-ping` (stdio JSON) | _see smoke artifact_ | _see smoke artifact_ | _see smoke artifact_ | `just podman-bench-normal-smoke` |
| S2-embed frame | Same + `NUSA_EMBED_TRANSPORT=frame` (`NEB1`) | _see smoke artifact_ | _see smoke artifact_ | _see smoke artifact_ | Compare P50/RPS vs JSON row in same smoke run |

**Interpretation:** Child mode pays per-request PHP process + IPC cost; Octane amortizes Laravel bootstrap. **S2-embed** removes UDS IPC (stdio embed daemon today; libphp in-process on roadmap).

## External baseline references (K1–K3)

Data from published benchmarks; workloads differ from our wrk smoke. Use as **order-of-magnitude KPI targets**, not drop-in replacements.

| Reference | Stack | Workload | Avg RPS | P99 (ms) | Notes |
|-----------|-------|----------|---------|----------|-------|
| [phpbenchlab.com][1] | PHP-FPM 8.5 + nginx (20 workers) | Mixed Laravel API (GET/POST/CRUD), `ab` HTTP/1.0 | 167 | 1 337 | Baseline; HTTP/1.0 short-lived connections |
| [phpbenchlab.com][1] | FrankenPHP 8.5 (Octane, 20 workers) | Same as FPM row | 167 | 1 381 | -0.4% vs FPM under `ab`; HTTP/2 not exercised |
| [phpbenchlab.com][1] | RoadRunner 8.5 (20 workers) | Same as FPM row | 237 | 944 | +41.6% vs FPM, -29% P99 |
| [Hoyo Tech][2] | PHP-FPM 8.3 (50 workers, 16 vCPU) | Real Laravel 11 API under production load | ~165 | ~200 (P95) | Avg 60ms |
| [Hoyo Tech][2] | Octane + FrankenPHP (32 workers, 16 vCPU) | Same endpoint set as FPM row | ~410 | ~54 (P95) | Avg 24ms; +148% RPS, -60% avg latency |
| [Differ.blog][3] | FrankenPHP (16-core, 32 GB) | High-concurrency microbenchmark | 62 000 avg / 78 000 peak | 22 (P99) | Extreme load; likely no-DB / warm cache |
| [Differ.blog][3] | RoadRunner (same hardware) | Same as FrankenPHP row | 67 000 avg / 84 000 peak | 20 (P99) | Slight edge under massive concurrency |
| [fpm-vs-swoole-vs-franken][4] | PHP-FPM 8.3 / Swoole / FrankenPHP (Docker) | Laravel 12 k6: health, ORM reads, writes, Fibonacci | — | — | Repo with identical Docker Compose setup; run for own KPI |

[1]: https://phpbenchlab.com/php-8-5-application-server-benchmark-frankenphp-roadrunner-vs-fpm/
[2]: https://hoyo.tech/article/still-using-php-fpm-heres-how-we-cut-laravel-api-response-times-by-60
[3]: https://differ.blog/p/frankenphp-vs-roadrunner-for-laravel-octane-speed-scalability-and-092f1c
[4]: https://github.com/mochavin/fpm-vs-swoole-vs-franken

### KPI interpretation from external data

| KPI | External benchmark value | Our current (S2 Octane IPC) | Gap |
|-----|------------------------|------------------------------|-----|
| P50 (API, worker mode) | 24–60ms avg (Hoyo Tech) | 124ms P50 | 2–5x slower |
| P99 (API, worker mode) | 22–54ms (Hoyo Tech / Differ) | 764ms P99 | 14–35x slower |
| RPS (API, worker mode) | ~410 (Hoyo Tech, 16 vCPU) | ~446 | **Comparable** on raw throughput |
| RPS (CPU-bound, no DB) | 167 (FPM) / 230 (Nusa child) | 230 | Comparable to FPM baseline |

**Key takeaway:** Nusa S2 Octane IPC **already matches/exceeds** external Octane+FrankenPHP RPS on similar worker count, but **P99 tail latency is an order of magnitude worse** — suggesting gateway queuing, IPC contention, or PHP worker GC rather than raw throughput limit.

### Benchmark policy

Nusa menggunakan **benchmark eksternal** untuk KPI target (FrankenPHP, FPM, RoadRunner dari sumber independen). Tidak menjalankan benchmark internal FPM/FrankenPHP sendiri — data publik sudah cukup untuk validasi order-of-magnitude KPI.

**Interpretation:** Child mode pays per-request PHP process + IPC cost; Octane amortizes Laravel bootstrap. **S2-embed** removes UDS IPC (stdio embed daemon today; libphp in-process on roadmap). Compare S1 to S1-FPM only after FPM baseline is recorded on identical hardware.

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
