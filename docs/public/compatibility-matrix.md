# Compatibility matrix (v1.0 target)

Honest support statement for GA. **Pilot today:** Alpine Linux musl, Laravel 11 minimal fixture, PHP 8.4 in CI image.

## Platform

| OS / libc | Role | Status |
|-----------|------|--------|
| Alpine Linux musl | **Authoritative CI / production target** | Supported |
| Linux glibc | Dev / optional deploy | Best-effort |
| Windows | Dev tests (stubs) | Not production |
| macOS | Dev tests (stubs) | Not production |

## PHP

| Version | Normal (`engine=child`) | Octane (`octane_workers>0`) |
|---------|-------------------------|-----------------------------|
| 8.4.x | CI image (`php84`) | CI fixture + E2E |
| 8.3.x | Best-effort | Best-effort |
| 8.2.x | Declared in driver `composer.json` | Requires validation |

## Laravel

| Version | Octane driver | E2E fixture |
|---------|---------------|-------------|
| 11.x | `nusa/octane` path package | `tests/fixtures/laravel-minimal` |
| 10.x | Intended (`illuminate/*` constraint) | Not CI-tested yet |

## Rust toolchain

| Component | Version |
|-----------|---------|
| `rust-version` (workspace) | 1.95 |
| Edition | 2024 |

## Engines (`nusa.toml`)

| `engine` | Production use |
|----------|----------------|
| `child` | **Default — supported** |
| `ffi` | Linux + PHP ZTS build required |
| `wasm` | **Dev only** — CLI exits unless `NUSA_ALLOW_WASM_STUB=1` |

## Laravel packages (priority)

| Package | Notes |
|---------|-------|
| `laravel/framework` | Fixture uses ^11.31 |
| `laravel/octane` | Required for worker bootstrap |
| `nusa/octane` | Path driver in `php-driver/` |

Full ecosystem guidance: [ecosystem/package-guidelines.md](ecosystem/package-guidelines.md).

## Verification commands

```bash
just podman-ci          # pre-merge (build image once: just podman-build)
just podman-ci-fast     # workspace-only Alpine gate
just podman-ci-e2e      # pre-GA: leak + IPC bench
```
