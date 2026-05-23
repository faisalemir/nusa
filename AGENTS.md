# AGENTS.md — AI agent entry point

Read this file first in every new session. Do **not** full-repository scan unless the task requires it.

## Read order

1. **This file** (`AGENTS.md`)
2. [`docs/ai/context-pack.md`](docs/ai/context-pack.md) — version, milestones, blockers, authoritative commands
3. [`docs/ai/routing.md`](docs/ai/routing.md) — which skill/rule applies
4. [`docs/ai/hotspots.md`](docs/ai/hotspots.md) — task → files (read only these crates)
5. [`.cursor/rules/nusa-standards.mdc`](.cursor/rules/nusa-standards.mdc) — quality gate (zero warnings, `just podman-ci`)
6. Source files for the specific hotspot only

Optional deep context (local, gitignored): `docs/plan/` blueprint 0–6.

## Documentation map (humans)

| Audience | Start here |
|----------|------------|
| Laravel developers | [`docs/public/laravel/README.md`](docs/public/laravel/README.md) · [`docs/public/quick-start.md`](docs/public/quick-start.md) |
| Deploy / operate | [`docs/public/README.md`](docs/public/README.md) · [runbook](docs/public/operations/runbook.md) |
| Contribute code | [`CONTRIBUTING.md`](CONTRIBUTING.md) → [`docs/contributor/`](docs/contributor/) |
| AI agents | This file → [`docs/ai/`](docs/ai/) |

## Hard rules

- Use **`just`** for project commands ([`justfile`](justfile)); do not invent recipes.
- **Authoritative CI for merge:** `just podman-ci` (Alpine musl), not host-only `just ci`.
- **No test gaming:** no silent skip, no `#[allow]` without justification, no green stub as production success.
- **English** for code comments, commits, and docs.
- **Do not claim GA / production-ready** until Phase 5 KPIs in [`docs/public/production-status.md`](docs/public/production-status.md) are met.

## Octane HTTP dispatch (P1 complete)

When `octane_pool` is initialized and `pool.is_ready()`, the gateway catch-all handler dispatches via `WorkerPool::handle_http_request` (not `engine.execute`). `/ready` fails closed if workers are configured but lack IPC transport. See [`crates/nusa-gateway/src/lib.rs`](crates/nusa-gateway/src/lib.rs) and [`docs/ai/hotspots.md`](docs/ai/hotspots.md).

**PHP driver:** Composer package **`nusa/octane`** in `php-driver/` — `NusaOctaneServiceProvider`, worker binary **`nusa-octane-worker`**. User guide: [`docs/public/laravel/php-driver.md`](docs/public/laravel/php-driver.md).

## Skills (project)

| Topic | Path |
|-------|------|
| Tests | `.cursor/skills/rust-test/SKILL.md` |
| Design | `.cursor/skills/rust-design-pattern/SKILL.md` |
| Docs rewrite / audit | `.cursor/skills/docs-expert/SKILL.md` |
| Performance | `.cursor/skills/m10-performance/SKILL.md` |

## When editing docs

Load **docs-expert** and verify every command against `justfile` and the codebase. See [`docs/ai/anti-patterns.md`](docs/ai/anti-patterns.md).
