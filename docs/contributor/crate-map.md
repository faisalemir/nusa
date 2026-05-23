# Crate map

Workspace members from root [`Cargo.toml`](../../Cargo.toml).

| Crate | Path | Responsibility |
|-------|------|----------------|
| `nusa-core` | `crates/nusa-core` | Domain types, VFS, tasks, backpressure helpers |
| `nusa-gateway` | `crates/nusa-gateway` | Axum app, routes, middleware, QUIC/TLS hooks |
| `nusa-engine-child` | `crates/nusa-engine-child` | PHP subprocess engine |
| `nusa-engine-ffi` | `crates/nusa-engine-ffi` | libphp FFI engine (`unsafe` allowed) |
| `nusa-engine-wasm` | `crates/nusa-engine-wasm` | WASM engine (stub in CLI today) |
| `nusa-octane-worker` | `crates/nusa-octane-worker` | Worker pool, spawn, state reset |
| `nusa-ipc` | `crates/nusa-ipc` | IPC protocol and transport (path dep) |
| `nusa-config` | `crates/nusa-config` | RuntimeConfig, hot reload |
| `nusa-security` | `crates/nusa-security` | Landlock, seccomp |
| `nusa-telemetry` | `crates/nusa-telemetry` | Metrics and export |
| `nusa-plugin-api` | `crates/nusa-plugin-api` | Plugin trait and registry |
| `nusa-cli` | `crates/nusa-cli` | `nusa` binary, dev/test subcommands |
| `nusa-benchmarks` | `benches` | Criterion benchmarks |

## Dependency direction (simplified)

```
nusa-cli → nusa-gateway, nusa-config, nusa-security, nusa-octane-worker, engines…
nusa-gateway → nusa-core, nusa-config, nusa-telemetry, …
nusa-octane-worker → nusa-ipc, nusa-core
engines → nusa-core, nusa-plugin-api (where applicable)
```

## Test locations

| Crate | Tests |
|-------|-------|
| Each library crate | `crates/<name>/tests/` |
| Workspace integration | `tests/integration/` |

## Hot-path files (Octane)

- `crates/nusa-gateway/src/lib.rs` — Octane branch in `handler`
- `crates/nusa-octane-worker/src/pool.rs` — pool API, spawns `nusa-octane-worker`
- `php-driver/bin/nusa-octane-worker` — PHP worker entrypoint
- `php-driver/src/NusaOctaneServiceProvider.php` — Laravel integration
- `crates/nusa-ipc/src/` — request/response with body
- `crates/nusa-cli/src/server.rs` — fail closed on pool init failure

See [`docs/ai/hotspots.md`](../ai/hotspots.md).
