# Testing

Nusa uses **cargo-nextest** via the [`justfile`](../../justfile).

## Authoritative gate

| Command | Role |
|---------|------|
| **`just podman-ci`** | fmt-check + clippy + tests in Alpine musl image — **pre-merge** |
| `just podman-test-full` | Full workspace tests in container |
| `just podman-test-pkg PACKAGE` | Single package in container |

Host commands (`just test`, `just test-fast`, `just ci`) are for **local iteration only**.

## Host development

```bash
just test-fast          # --no-run, retries 0
just test-crate gateway
just test-new           # new/extended test files
```

## Live PHP tests

```bash
just podman-test-live
```

Requires PHP/Laravel fixtures in the image — not part of default fast CI unless configured.

## Test categories (by file naming)

| Pattern | Purpose |
|---------|---------|
| `*_test.rs` | Unit/integration per crate |
| `security_*`, `*_security_*` | Landlock, seccomp, injection |
| `concurrency_*` | Races, parallel load |
| `resource_exhaustive_*` | Limits, OOM paths |
| `stress_decision_*` | Decision tables under load |
| `tests/integration/` | Cross-crate workspace tests |

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

Some crates (e.g. `nusa-cli`, `nusa-engine-ffi`) may be excluded in container recipes — if you change them, validate explicitly (see `justfile` `podman-test` filters).

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
- `.cursor/skills/rust-test/SKILL.md`
- `nusa-standards.mdc` — Production Test Integrity section
