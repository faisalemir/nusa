# RFC: Production readiness phases

Status: **Accepted as execution plan** (documentation and implementation tracking).

Last updated: **2026-05-23**

## Goal

Reach **GA v1.0** for Laravel on Alpine musl with honest docs, wired Octane HTTP path, E2E CI, and benchmark KPIs.

## Phase summary

| Phase | Deliverable | Implementation | Verification (Alpine) |
|-------|-------------|----------------|------------------------|
| **P0** | Public docs for **Laravel developers** (`docs/public/`, `docs/public/laravel/*`); contributor / AI | **Done** | N/A |
| **P1** | Gateway Octane dispatch; `/ready` fail-closed; IPC body; spawn fail-closed | **Done** | `just podman-ci` (sector S02) |
| **P2** | Laravel fixture; tiered Podman gates; leak + IPC bench | **Done** (code + recipes) | `just podman-ci-e2e` must be green on release branch |
| **P3** | Normal Mode benchmarks | Templates in `docs/benchmarks/` | Fill numbers before GA tag |
| **P4** | GA v1.0 release process | Checklist in compatibility matrix + CHANGELOG | Signed `podman-ci` + `podman-ci-e2e` logs |
| **P5** | Blueprint phase 6 (advanced) | [`post-ga-blueprint.md`](../post-ga-blueprint.md) | Deferred |

## Podman gate model (2026-05)

Tiered gates avoid rebuilding the test image on every run. See [`testing.md`](../testing.md).

| Recipe | Use |
|--------|-----|
| `just podman-build` | When Dockerfile, `Cargo.lock`, or Laravel fixture deps change |
| `just podman-ci-fast` | Pre-merge for Rust-only changes (workspace, no E2E) |
| `just podman-ci` | **Pre-merge authoritative** — workspace + Laravel E2E smoke |
| `just podman-ci-e2e` | Pre-GA — adds leak 10k (`octane_leak_suite` only) + IPC bench |
| `just podman-ci-rebuild` | Image rebuild + `podman-ci` |

Composer at runtime is conditional via `dockerfiles/podman-laravel-fixture.sh` (skips install when `vendor/` is valid).

## P1 technical acceptance

| # | Criterion | Evidence |
|---|-----------|----------|
| 1 | HTTP uses pool when `octane_pool` ready | `nusa-gateway/src/lib.rs`, `octane_dispatch_test.rs` |
| 2 | CLI exits if `octane_workers > 0` and pool not ready | `nusa-cli/src/main.rs`, `octane_init_test.rs` |
| 3 | `/ready` → 503 when pool required but not ready | `octane_dispatch_test.rs` |
| 4 | Integration tests prove dispatch branch | S02 tests + fake IPC (`test_fake_ipc.rs`) |

**Sign-off gate:** `just podman-ci` green.

## P2 technical acceptance

| # | Criterion | Evidence |
|---|-----------|----------|
| 1 | Minimal Laravel app | `tests/fixtures/laravel-minimal/` |
| 2 | Live pool + IPC + routes | `crates/nusa-e2e-tests/tests/laravel_live.rs` |
| 3 | Gateway → Laravel (engine not called) | `laravel_gateway_octane_dispatch_integration` |
| 4 | Leak suite | `octane_leak_suite.rs` (`NUSA_LEAK_REQUESTS`) |
| 5 | Trace on IPC | `trace_propagation_test.rs` |
| 6 | CI recipes | `just podman-ci`, `just podman-ci-e2e` in `justfile` |

**Sign-off gate:** `just podman-ci-e2e` green (includes artisan contract script + IPC bench smoke).

### P2 optional (not blocking v0.1.0 code complete)

- Published IPC P99 and Normal Mode wrk/k6 numbers (P3) — scripts in `tests/load/`

## Test sector tracking (S02 / S14 / S15)

Aligned with [`testing.md`](../testing.md).

| Sector | Theme | Status |
|--------|-------|--------|
| **S02** | Octane dispatch + CLI pool init | **Implemented** — pending Alpine CI log on branch |
| **S14** | Plugin exhaustive suites | **Implemented** — six test binaries under `nusa-plugin-api` |
| **S15** | Laravel live + leak | **Implemented** — smoke in `podman-ci`; leak 10k in `podman-ci-e2e` |

## P3–P4 remaining work

1. Run wrk/k6 on release hardware; record in `docs/benchmarks/normal-mode-report.md`
2. Tag release per [`release-checklist.md`](../release-checklist.md) (`.github/workflows/release.yml` + SBOM)
3. Flip `docs/public/production-status.md` to GA criteria when KPIs + gates are signed

## References

- Testing gates and sector map: [`testing.md`](../testing.md)
- AI hotspots: [`docs/ai/hotspots.md`](../../ai/hotspots.md)
- Public status: [`docs/public/production-status.md`](../../public/production-status.md)
- Blueprint (optional local): `docs/plan/` (gitignored on some clones)

## Decision log

| Date | Decision |
|------|----------|
| 2026-05 | v0.1.0 is pre-GA; README must not claim Phase 1–4 “complete” without benchmark KPIs |
| 2026-05 | Authoritative CI = `just podman-ci` (live mount); not `just podman-test` alone |
| 2026-05 | Docs split: public / contributor / AI |
| 2026-05-23 | Tiered Podman gates; no `podman-build` on every `podman-ci` |
| 2026-05-23 | S02/S14/S15 test implementations accepted; verification = green Alpine logs |
| 2026-05-23 | `podman-ci-e2e` runs leak binary only (no second full E2E pass) |
