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
| Deploy / operate | [`README.md`](README.md) → [`docs/public/`](docs/public/) |
| Contribute code | [`CONTRIBUTING.md`](CONTRIBUTING.md) → [`docs/contributor/`](docs/contributor/) |
| AI agents | This file → [`docs/ai/`](docs/ai/) |

## Hard rules

- Use **`just`** for project commands ([`justfile`](justfile)); do not invent recipes.
- **Authoritative CI for merge:** `just podman-ci` (Alpine musl), not host-only `just ci`.
- **No test gaming:** no silent skip, no `#[allow]` without justification, no green stub as production success.
- **English** for code comments, commits, and docs.
- **Do not claim GA / production-ready** until Phase 5 KPIs in [`docs/public/production-status.md`](docs/public/production-status.md) are met.

## Current implementation blocker (check before Octane work)

When `octane_workers > 0`, the worker pool is initialized in CLI but the **HTTP handler still calls `engine.execute` only** — it does not dispatch to `WorkerPool`. See [`docs/ai/hotspots.md`](docs/ai/hotspots.md) and P1 in the production plan.

## Skills (project)

| Topic | Path |
|-------|------|
| Tests | `.cursor/skills/rust-test/SKILL.md` |
| Design | `.cursor/skills/rust-design-pattern/SKILL.md` |
| Docs rewrite / audit | `.cursor/skills/docs-expert/SKILL.md` |
| Performance | `.cursor/skills/m10-performance/SKILL.md` |

## When editing docs

Load **docs-expert** and verify every command against `justfile` and the codebase. See [`docs/ai/anti-patterns.md`](docs/ai/anti-patterns.md).
