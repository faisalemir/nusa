# AI context pack

Single-page facts for Nusa PHP Runtime. Updated for documentation rewrite; verify against code before claiming behavior changed.

## Product

- **Name:** Nusa PHP Runtime
- **Version:** `0.1.0` (workspace `Cargo.toml`)
- **Target production OS:** Alpine Linux **musl** (see `dockerfiles/nusa-test-runner.Dockerfile`)
- **Role:** Rust orchestrator (gateway, security, observability) + PHP execution (child / FFI / Octane workers)

## Execution modes (design)

| Mode | Config | Behavior (intended) |
|------|--------|---------------------|
| **Normal** | `octane_workers = 0`, `engine = child` (default) | Per-request PHP via `PhpEngine` |
| **Octane** | `octane_workers > 0` | Long-lived Laravel workers via IPC (`WorkerPool`) |

**Implementation note (v0.1.0):** CLI initializes `WorkerPool` when `octane_workers > 0`, but the gateway **catch-all handler still uses `state.engine.execute` only**. Octane dispatch in HTTP is **not complete** until gateway routes to the pool.

## Milestones (blueprint)

| Phase | Scope | Doc status |
|-------|-------|------------|
| 0–4 | Foundation → advanced features | Partially implemented in Rust |
| 5 | GA: benchmarks, matrix, signed release | **Planned** — not v1.0 yet |

See [`docs/public/production-status.md`](../public/production-status.md) for an honest scorecard.

## Workspace crates

`nusa-core`, `nusa-gateway`, `nusa-engine-child`, `nusa-engine-ffi`, `nusa-engine-wasm`, `nusa-ipc` (dependency), `nusa-octane-worker`, `nusa-config`, `nusa-security`, `nusa-telemetry`, `nusa-plugin-api`, `nusa-cli`, `benches` (`nusa-benchmarks`).

## Authoritative commands (`just`)

| Task | Recipe |
|------|--------|
| Format | `just fmt` / `just fmt-check` |
| Lint | `just lint` |
| Host tests (dev) | `just test-fast` |
| **Pre-merge CI** | **`just podman-ci`** |
| Full Alpine tests | `just podman-test-full` |
| Single crate test | `just test-crate gateway` |
| Security | `just security` |
| API docs | `just docs` |
| Benchmarks | `just bench` / `just bench-fast ipc_latency_bench` |

Host `just ci` is **not** sufficient alone for merge.

## Run binary (dev)

```bash
cargo build -p nusa-cli --release
cargo run -p nusa-cli -- --config config.toml
# Subcommands: dev, test, deploy, rollback (see nusa-cli/src/lib.rs)
```

Copy [`config.toml.example`](../config.toml.example) to `nusa.toml` (or pass `--config`).

## Gateway routes (verified in code)

- `GET /health`, `GET /ready`
- `GET /metrics`
- `GET /ws`, `GET /sse`
- `POST /api/tasks`, `GET /api/tasks/{task_id}/status`
- `GET /static/{*path}`
- `GET|POST /{*path}` → PHP handler

There is **no** `/admin/recycle-all` route in `nusa-gateway` today — do not document it as available.

## Engines (CLI `main.rs`)

| `engine` | v0.1.0 behavior |
|----------|-----------------|
| `child` | Default; spawns PHP child processes |
| `ffi` | Linux + PHP ZTS headers; stub on other platforms |
| `wasm` | **`WasmEngine::stub()`** in CLI — not production PHP WASM |

## PHP / Laravel driver

- Composer package skeleton: `php-driver/`
- Worker script: `php-driver/bin/octane-rust-worker`
- Requires Laravel app with `vendor/` and `bootstrap/app.php` for real Octane workers

## Quality rules (summary)

- `#![deny(unsafe_code)]` except `nusa-engine-ffi` with `// SAFETY:` comments
- No `unwrap()` in library code
- Production tests: fail closed on Linux security enforcement — see `nusa-standards.mdc`
