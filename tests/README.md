# Tests

Nusa uses **cargo-nextest** via [`justfile`](../justfile). See [Contributor testing guide](../docs/contributor/testing.md).

## Layout

| Path | Purpose |
|------|---------|
| `tests/integration/` | Workspace-level integration tests |
| `crates/*/tests/` | Per-crate integration and extended suites |

Naming conventions include `security_*`, `concurrency_*`, `resource_exhaustive_*`, `stress_decision_*`.

## Commands

```bash
just test-fast              # host, no rebuild
just test-crate gateway     # single crate
just podman-ci              # authoritative (Alpine musl)
just podman-test-full       # full container test run
just podman-test-live       # PHP/Laravel live scenarios (when available)
```

## Integrity rules

- No silent skip on Linux enforcement failures
- Stub paths need `// STUB_CONTRACT:` and Alpine coverage for real behavior
- Pre-merge: **`just podman-ci`**, not host-only `just ci`

## AI agents

See [`docs/ai/context-pack.md`](../docs/ai/context-pack.md) for test counts and gates — do not cite stale README numbers.
