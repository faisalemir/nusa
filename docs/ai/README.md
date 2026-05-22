# AI agent documentation

Minimize cold-start cost: read in order, then open **hotspot files only**.

## Required reading

| Order | File | Purpose |
|-------|------|---------|
| 1 | [`AGENTS.md`](../../AGENTS.md) | Entry rules and doc map |
| 2 | [context-pack.md](context-pack.md) | Facts: version, phases, blockers, commands |
| 3 | [routing.md](routing.md) | Meta-cognition and skill selection |
| 4 | [hotspots.md](hotspots.md) | Task → crate → path |
| 5 | [anti-patterns.md](anti-patterns.md) | Forbidden shortcuts |

## Optional

- Blueprint (gitignored on clone): `docs/plan/` — philosophy, milestones M0–M6
- Human contributor detail: [`docs/contributor/`](../contributor/)

## Do not

- Run exploratory listing of entire `crates/` without a task filter
- Trust old README phase tables (58 tests) — use `just podman-test-full` counts
- Assume Octane HTTP path is wired — verify `nusa-gateway/src/lib.rs` `handler`
