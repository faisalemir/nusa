# RFC: Production readiness phases

Status: **Accepted as execution plan** (documentation and implementation tracking).

## Goal

Reach **GA v1.0** for Laravel on Alpine musl with honest docs, wired Octane HTTP path, E2E CI, and benchmark KPIs.

## Phases

| Phase | Deliverable | Status (repo) |
|-------|-------------|---------------|
| **P0** | Greenfield docs: public / contributor / AI (`AGENTS.md`, `docs/`) | In progress |
| **P1** | Gateway Octane dispatch; `/ready` fail-closed; IPC body; spawn fail-closed | Not started |
| **P2** | Laravel fixture; `podman-test-live` E2E; M2 KPIs | Not started |
| **P3** | Criterion Normal Mode benchmarks | Not started |
| **P4** | GA v1.0 release process | Not started |
| **P5** | Blueprint phase 6 (advanced) | Deferred post-GA |

## P1 technical acceptance

1. When `octane_pool` is `Some` and workers ready, HTTP handler uses pool path, not `engine.execute`.
2. When `octane_workers > 0` and pool init fails, process exits non-zero (CLI).
3. `/ready` returns 503 if Octane required but pool not ready.
4. Integration tests in `nusa-gateway` prove dispatch branch.

## P2 technical acceptance

1. Minimal Laravel app in repo or test image
2. `just podman-test-live` exercises real `vendor/` bootstrap
3. Documented pass/fail in CI logs

## References

- AI hotspots: [`docs/ai/hotspots.md`](../../ai/hotspots.md)
- Public status: [`docs/public/production-status.md`](../../public/production-status.md)
- Blueprint (local, gitignored): `docs/plan/`

## Decision log

| Date | Decision |
|------|----------|
| 2026-05 | v0.1.0 is pre-GA; README must not claim Phase 1–4 “complete” |
| 2026-05 | Authoritative CI = `just podman-ci` |
| 2026-05 | Docs split: public / contributor / AI |
