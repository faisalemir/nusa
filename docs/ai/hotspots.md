# AI hotspots — where to edit

Read **only** these paths for the listed task. Line numbers drift; grep for symbols.

## P1 blocker: Octane HTTP dispatch

| File | Symbol / area | Issue |
|------|---------------|--------|
| [`crates/nusa-gateway/src/lib.rs`](../../crates/nusa-gateway/src/lib.rs) | `handler`, `AppState` | `octane_pool` in state; handler calls `engine.execute` only |
| [`crates/nusa-octane-worker/src/pool.rs`](../../crates/nusa-octane-worker/src/pool.rs) | `WorkerPool` | Pool init, worker lifecycle |
| [`crates/nusa-cli/src/main.rs`](../../crates/nusa-cli/src/main.rs) | pool init | On failure: warns and sets `None` — should fail closed when workers > 0 |
| [`crates/nusa-ipc/src/transport.rs`](../../crates/nusa-ipc/src/transport.rs) | request/response | May need `body` on request for HTTP proxy |

**Target behavior:** If `octane_pool` is `Some` and ready → `handle_http_request` (or equivalent); else engine path or 503 on `/ready`.

## Gateway

| Concern | Path |
|---------|------|
| Router, state | `crates/nusa-gateway/src/lib.rs` |
| Middleware | `crates/nusa-gateway/src/middleware.rs` |
| WebSocket | `crates/nusa-gateway/src/websocket.rs` |
| SSE | `crates/nusa-gateway/src/sse.rs` |
| QUIC / TLS / ACME | `crates/nusa-gateway/src/quic.rs`, `acme.rs` |
| Tests | `crates/nusa-gateway/tests/` |

## Core domain

| Concern | Path |
|---------|------|
| Tasks | `crates/nusa-core/src/task.rs` |
| VFS | `crates/nusa-core/src/vfs.rs` |
| Guards / rate limit | `crates/nusa-core/src/guards.rs` |

## Engines

| Engine | Path |
|--------|------|
| Child | `crates/nusa-engine-child/src/engine.rs` |
| FFI | `crates/nusa-engine-ffi/src/engine.rs` |
| WASM | `crates/nusa-engine-wasm/src/engine.rs` |

## Security

| Concern | Path |
|---------|------|
| Landlock | `crates/nusa-security/src/landlock.rs` |
| Seccomp | `crates/nusa-security/src/seccomp.rs` |
| Tests | `crates/nusa-security/tests/security_enforcement_test.rs` |

## Config keys

| Key | Source |
|-----|--------|
| `octane_workers`, `engine`, bind | `crates/nusa-config/src/lib.rs` |
| Example TOML | `config.toml.example` |

## Integration tests (workspace)

| Area | Path |
|------|------|
| Full lifecycle | `tests/integration/full_lifecycle_test.rs` |
| Podman live (PHP) | `just podman-test-live` — not default `podman-test` |

## PHP driver

| Artifact | Path |
|----------|------|
| Octane worker | `php-driver/bin/octane-rust-worker` |
| Composer package | `php-driver/composer.json` |
