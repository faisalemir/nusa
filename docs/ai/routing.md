# AI routing — skills and entry layer

Maps user intent to project skills. Full framework: [`.cursor/rules/nusa-standards.mdc`](../../.cursor/rules/nusa-standards.mdc).

## Meta-cognition layers

```
Layer 3: Domain (WHY)     → cloud-native, 12-factor, Alpine musl gate
Layer 2: Design (WHAT)    → rust-design-pattern, crate boundaries
Layer 1: Rust (HOW)       → m01–m15 actionbook skills
```

## Signal → skill

| User signal | Primary skill(s) |
|-------------|-------------------|
| Compiler E0xxx | `m01-ownership` … trace up to design |
| New feature / API | `rust-design-pattern` → `m06-error-handling` → `rust-test` |
| Tests / CI green | `rust-test` + **Production Test Integrity** → `just podman-ci` |
| Gateway / IPC / worker hot path | `m10-performance` + `rust-test` + benches |
| Docs rewrite | `docs-expert` — validate `justfile` |
| Security (Landlock, seccomp) | `rust-test` + `nusa-security` tests — fail closed on Linux |

## Task → first files (before broad search)

| Task | Open first |
|------|------------|
| HTTP / middleware / TLS | `crates/nusa-gateway/src/lib.rs`, `middleware.rs` |
| Octane / workers | `crates/nusa-octane-worker/src/pool.rs`, `crates/nusa-gateway/src/lib.rs` |
| IPC protocol | `crates/nusa-ipc/src/` |
| Config / env | `crates/nusa-config/src/lib.rs`, `config.toml.example` |
| CLI startup | `crates/nusa-cli/src/main.rs` |
| Sandbox | `crates/nusa-security/src/` |
| VFS / tenant | `crates/nusa-core/src/vfs.rs`, `task.rs` |

## Phase work (production plan)

| Phase | Focus |
|-------|--------|
| P0 | Docs only — this tree |
| P1 | Gateway Octane dispatch, `/ready` fail-closed, IPC body, spawn fail-closed |
| P2 | Laravel fixture + E2E in `just podman-test-live` |
| P3 | Criterion Normal Mode benchmarks |
| P4 | GA v1.0 |

Do not implement P5 (blueprint 6 advanced) before P4 unless explicitly requested.

## Commit / PR hygiene

- Prefix: `feat:`, `fix:`, `test:`, `docs:`, `chore:`
- English messages
- User must ask before `git commit` or `gh pr create`
