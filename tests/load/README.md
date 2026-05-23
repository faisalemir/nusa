# Load tests (P3)

k6 and wrk scenarios for Normal Mode vs FPM baselines. Run on the **same host** as Nusa for comparable numbers.

## Prerequisites

- Nusa listening on `:8080` with `engine = child`, `octane_workers = 0` (or Octane with fixture `code_dir`)
- Optional FPM/nginx baseline on another port for comparison
- Tools: [wrk](https://github.com/wg/wrk), [k6](https://k6.io/) (host or `apk add wrk` in Alpine)

## wrk (quick)

```bash
wrk -t4 -c100 -d30s http://127.0.0.1:8080/
```

Capture output for the report:

```bash
sh tests/load/record-wrk.sh http://127.0.0.1:8080/ nusa-child
```

Paste P50/P99 into [`docs/benchmarks/normal-mode-report.md`](../../docs/benchmarks/normal-mode-report.md).

## k6

| Script | Purpose |
|--------|---------|
| [`normal-static.js`](normal-static.js) | Simple GET load (`K6_TARGET`, `K6_VUS`, `K6_DURATION`) |
| [`laravel-fixture.js`](laravel-fixture.js) | Fixture routes `/nusa-ping`, `POST /nusa-echo` (`K6_BASE`) |

```bash
k6 run tests/load/normal-static.js
k6 run -e K6_BASE=http://127.0.0.1:8080 tests/load/laravel-fixture.js
```

## Alpine note

The test image includes `wrk` after `just podman-build` (Dockerfile). On an older image without wrk, install on the benchmark host:

```bash
apk add wrk   # Alpine
```

CI proof for correctness remains `just podman-ci` / `just podman-ci-e2e`; load tests are **P3 evidence** for GA tables.

`just podman-bench-normal-smoke` sets `NUSA_SKIP_SECCOMP=1` by default (landlock still on). For production-like runs on bare Alpine, unset it and use `NUSA_BENCH_RELEASE=1`.

## Sign-off

Results belong in `docs/benchmarks/normal-mode-report.md` before GA v1.0.0 tag. See [`docs/contributor/release-checklist.md`](../../docs/contributor/release-checklist.md).
