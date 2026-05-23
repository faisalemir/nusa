# Architecture

High-level design of Nusa PHP Runtime v0.1.0.

## Request path (Normal mode)

```mermaid
sequenceDiagram
    participant Client
    participant Gateway as nusa_gateway
    participant Engine as PhpEngine
    participant PHP as PHP_child_or_ffi

    Client->>Gateway: HTTP request
    Gateway->>Gateway: middleware rate_limit circuit
    Gateway->>Gateway: plugins run_pre_exec
    Gateway->>Engine: execute RequestContext
    Gateway->>Gateway: plugins run_post_exec
    Engine->>PHP: run script
    PHP-->>Engine: response
    Engine-->>Gateway: EngineResponse
    Gateway-->>Client: HTTP response
```

Entry: [`crates/nusa-gateway/src/lib.rs`](../../crates/nusa-gateway/src/lib.rs) `handler` → plugins → `state.engine.execute(ctx)` when `octane_workers = 0`.

## Request path (Octane)

```mermaid
sequenceDiagram
    participant Client
    participant Gateway
    participant Pool as WorkerPool
    participant Worker as PHP_Octane_worker

    Client->>Gateway: HTTP request
    Gateway->>Gateway: plugins run_pre_exec
    Note over Gateway: branch when octane_pool ready
    Gateway->>Pool: handle_http_request
    Gateway->>Gateway: plugins run_post_exec
    Pool->>Worker: IPC
    Worker-->>Pool: response
    Pool-->>Gateway: response
    Gateway-->>Client: HTTP response
```

When `octane_workers > 0` and `pool.is_ready()`, the gateway uses **`WorkerPool::handle_http_request`** instead of `engine.execute`. See [`crates/nusa-cli/src/server.rs`](../../crates/nusa-cli/src/server.rs).

## PHP driver (`nusa/octane`)

| Artifact | Path |
|----------|------|
| Composer package | `php-driver/composer.json` (`name`: `nusa/octane`) |
| Service provider | `php-driver/src/NusaOctaneServiceProvider.php` |
| Worker binary | `php-driver/bin/nusa-octane-worker` |

The pool resolves the worker script under `code_dir` (see `crates/nusa-octane-worker/src/pool.rs`). User-facing install guide: [`docs/public/laravel/php-driver.md`](../public/laravel/php-driver.md).

## Process lifecycle (CLI)

1. Telemetry init
2. Load config (`nusa-config`, hot reload)
3. Construct `PhpEngine` from `engine` kind
4. Apply Landlock + seccomp (`nusa-security`)
5. Build gateway `AppState` (plugins, circuit breakers, tenants, tasks, WS, SSE, metrics, **octane_pool**)
6. Optional: Redis broadcast, experimental TLS/QUIC (feature flags)
7. `BlueGreenDeployer` + `axum::serve` on `bind` (see `nusa deploy` / `nusa rollback`)

## Plugins

`PluginRegistry` runs **`run_pre_exec`** before PHP/Octane execution and **`run_post_exec`** after a successful response. Register plugins on the registry passed to `nusa_gateway::app()`.

## Major components

| Layer | Crate | Role |
|-------|-------|------|
| Edge | `nusa-gateway` | HTTP, WS, SSE, static, health, metrics |
| Domain | `nusa-core` | Request context, VFS, tasks, guards |
| Execution | `nusa-engine-child`, `nusa-engine-ffi`, `nusa-engine-wasm` | `PhpEngine` trait |
| Workers | `nusa-octane-worker`, `nusa-ipc` | Persistent Laravel workers |
| Policy | `nusa-security` | Landlock, seccomp |
| Config | `nusa-config` | TOML + env |
| Observability | `nusa-telemetry` | Metrics, tracing export |
| Plugins | `nusa-plugin-api` | Extension registry |
| Binary | `nusa-cli` | `nusa` command |

## Multi-tenancy

`TenantRegistry`, per-tenant rate limits, and tenant circuit breakers live in gateway state. Requests carry tenant identity in `RequestContext` when configured.

## Async tasks

`POST /api/tasks` offloads work via `nusa-core` `TaskManager` — separate from synchronous PHP page handling.

## Deployment artifacts

- **CI/test image:** `dockerfiles/nusa-test-runner.Dockerfile`
- **Alpine variant:** `dockerfiles/nusa-test-alpine.Dockerfile`
- **Benchmarks:** `benches/` (`nusa-benchmarks`)

## Related docs

- [crate-map.md](crate-map.md)
- [Public production status](../public/production-status.md)
