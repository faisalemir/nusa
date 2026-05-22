# Testing

Nusa uses **cargo-nextest** via the [`justfile`](../../justfile).

## Authoritative gate (tiered)

| Command | When | Role |
|---------|------|------|
| `just podman-build` | Dockerfile / `Cargo.lock` / Laravel deps changed | Build `nusa-test-runner` image (**not** every CI run) |
| **`just podman-ci-fast`** | Daily / Rust-only loop | fmt-check + Alpine clippy + workspace tests |
| **`just podman-ci`** | **Pre-merge** | fast tier + Laravel E2E smoke (`nusa-e2e-tests`) |
| **`just podman-ci-e2e`** | Pre-GA / nightly | workspace + E2E + leak 10k + IPC bench (leak-only filter; no duplicate full E2E) |
| `just podman-ci-rebuild` | After image inputs change | `podman-build` then `podman-ci` |

Host commands (`just test`, `just test-fast`, `just ci`) are for **local iteration only**.

| Command | Validates host working tree? |
|---------|------------------------------|
| `just podman-ci`, `podman-ci-fast`, `podman-test-live`, `podman-test-workspace` | **Yes** (live mount) |
| `just podman-test` | **No** — image-baked source at build time |

## Host development

```bash
just test-fast          # --no-run, retries 0
just test-crate gateway
just test-new           # new/extended test files
```

## Targeted Alpine runs

```bash
just podman-test-workspace    # ~1593 workspace tests, no E2E
just podman-test-e2e          # nusa-e2e-tests only
just podman-test-live         # workspace + E2E
just podman-test-pkg gateway  # single crate
just podman-test-laravel-leak      # octane_leak_suite only (10k in CI)
just podman-test-laravel-leak-dev  # NUSA_LEAK_REQUESTS=1000
```

Fixture setup: [`dockerfiles/podman-laravel-fixture.sh`](../../dockerfiles/podman-laravel-fixture.sh) — skips `composer install` when `vendor/` is already valid.

## Test categories (by file naming)

| Pattern | Purpose |
|---------|---------|
| `*_test.rs` | Unit/integration per crate |
| `security_*`, `*_security_*` | Landlock, seccomp, injection |
| `concurrency_*` | Races, parallel load |
| `resource_exhaustive_*` | Limits, OOM paths |
| `stress_decision_*` | Decision tables under load |
| `decision_exhaustive_*` | Config/guard combinatorics |
| `*_e2e_test.rs`, `*_domain_test.rs` | Stack or business-path depth |
| `tests/integration/` | Cross-crate workspace tests |

## Test sectors (S01–S18)

Full registry: `.cursor/skills/rust-test/SKILL.md` → **Nusa Test Sector Registry**.

### Sector → gate map

| ID | Sector | Primary tests | Alpine gate |
|----|--------|---------------|-------------|
| S01 | Gateway HTTP / middleware | `gateway_*_test`, `middleware_*` | `podman-ci-fast` |
| **S02** | **Octane HTTP dispatch + CLI pool init** | `octane_dispatch_test`, `octane_init_test`, `test_fake_ipc` | **`podman-ci`** |
| S03–S07 | WS/SSE, TLS/QUIC, static, breaker, blue-green | `*_e2e_test` per area | `podman-ci-fast` |
| S08–S09 | Octane worker + IPC | `nusa-octane-worker/tests/*`, `nusa-ipc/tests/*` | `podman-ci-fast` + Laravel for live IPC |
| S10–S11 | Core + config / hot reload | `nusa-core`, `nusa-config` tests | `podman-ci-fast` |
| S12 | Landlock / seccomp | `security_enforcement_test`, `security_*` | `podman-ci-fast` (fail closed on Linux) |
| S13 | Telemetry | `nusa-telemetry/tests/*` | `podman-ci-fast` |
| **S14** | **Plugins** | `plugin_{domain,security,decision_exhaustive,resource_exhaustive,stress_decision}_*` | **`podman-ci-fast`** |
| **S15** | **Laravel live + leak** | `laravel_live`, `octane_leak_suite`, `trace_propagation` | **`podman-ci`** (smoke) + **`podman-ci-e2e`** (leak 10k) |
| S16–S17 | Engines + CLI | engine crates; `octane_init_test` | `podman-test-pkg` when touched; FFI excluded default |
| S18 | Workspace integration | `tests/integration/*` | `podman-test-full` |

### Sector sign-off (implementation vs verification)

| ID | Implementation in repo | Verification pending |
|----|------------------------|-------------------|
| **S02** | **Done** — gateway dispatch/readiness, fake IPC pool, `init_octane_pool` fail-closed | Green `just podman-ci` on current branch |
| **S14** | **Done** — six plugin test binaries (domain, security, exhaustive, stress) | Green `just podman-ci-fast` |
| **S15** | **Mostly done** — fixture routes (GET/POST/query/counter), pool IPC, gateway→Laravel, leak suite | Green `just podman-ci` + `podman-ci-e2e`; optional session/middleware E2E rows below |
| S12 | Done (enforcement fail-closed) | Optional extra seccomp stress/decision combinatorics |

### Remaining gaps (test plan)

| Gap | Owner / gate | Notes |
|-----|--------------|-------|
| S15 session/middleware E2E | P2+ / pre-GA | No dedicated test for Laravel session cookie round-trip or custom middleware stack; fixture uses `SESSION_DRIVER=array` |
| `just podman-ci-e2e` green log | Release sign-off | Leak 10k + IPC bench must pass once on release hardware |
| S16 `nusa-engine-ffi` | Manual | Excluded from default Podman workspace run |
| S17 `nusa-cli` full suite | `podman-test-pkg cli` | Excluded from default Podman workspace run |

### Key artifacts (S02 / S14 / S15)

| Sector | Paths |
|--------|-------|
| S02 | `crates/nusa-gateway/tests/octane_dispatch_test.rs`, `crates/nusa-cli/tests/octane_init_test.rs`, `crates/nusa-cli/src/octane_pool.rs`, `crates/nusa-octane-worker/src/test_fake_ipc.rs` |
| S14 | `crates/nusa-plugin-api/tests/plugin_*.rs` (6 binaries) |
| S15 | `tests/fixtures/laravel-minimal/`, `crates/nusa-e2e-tests/tests/laravel_live.rs`, `octane_leak_suite.rs`, `trace_propagation_test.rs` |

## Production test integrity

Forbidden:

- Silent skip on Linux enforcement failure
- `assert!(true)` padding
- Stub `is_ok()` when production contract is `Err`
- `#[ignore]` without `// REASON:` and scope

Required for stubs:

```rust
// STUB_CONTRACT: proves X on non-Linux; Alpine must run enforcement test Y
```

## Excluded from default Podman run

`nusa-cli` and `nusa-engine-ffi` are excluded in `justfile` workspace recipes — if you change them, run `just podman-test-pkg cli` (or validate FFI on a Linux host with ZTS PHP).

## Coverage

```bash
just test-coverage
```

## Workspace integration tests

- `tests/integration/full_lifecycle_test.rs`
- `tests/integration/hot_reload_e2e_test.rs`
- Others under `tests/integration/`

## Related

- [`tests/README.md`](../../tests/README.md)
- [Production readiness phases](rfc/production-readiness.md) — P0–P5 + sector tracking
- `.cursor/skills/rust-test/SKILL.md`
- `.cursor/rules/nusa-standards.mdc` — Production Test Integrity section
