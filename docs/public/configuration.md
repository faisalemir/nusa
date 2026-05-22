# Configuration reference

Nusa is operated through a **small, explicit configuration surface**—`nusa.toml` plus environment overrides—because platform teams should not reverse-engineer PHP ini files to understand concurrency, sandbox paths, or worker topology.

Configuration loads through **figment**: defaults → TOML file → `NUSA_*` environment variables. Hot reload swaps the active config with **`ArcSwap`** when `hot_reload = true`, so you can change limits without rebuilding images.

Implementation source: [`crates/nusa-config/src/lib.rs`](../../crates/nusa-config/src/lib.rs).  
Example file: [`config.toml.example`](../../config.toml.example).

---

## Philosophy: three knobs that matter

Most runtimes scatter policy across nginx, php-fpm pool files, `.env`, and Supervisor. Nusa centralizes **execution**, **capacity**, and **sandbox paths** in one file:

1. **How PHP runs** — `engine`, `octane_workers`, recycle limits  
2. **How hard the gateway protects itself** — `max_workers`, `timeout_ms`  
3. **Where the kernel allows filesystem access** — `code_dir`, `tmp_dir`, `vfs_root`

Everything else supports those decisions.

---

## Complete key reference

| Key | Type | Default | What it controls |
|-----|------|---------|------------------|
| `engine` | `child` \| `ffi` \| `wasm` | `child` | Which `PhpEngine` implementation executes PHP in **Normal mode** |
| `max_workers` | usize | `4` | Gateway concurrency and backpressure ceiling (**must be > 0**) |
| `timeout_ms` | u64 | `30000` | Per-request upstream timeout in milliseconds |
| `wasm_memory_mb` | u64 | `256` | WASM store limit when `engine = "wasm"` |
| `vfs_root` | string | `/app/public` | Virtual filesystem root for script resolution |
| `code_dir` | string | `/app/public` | Laravel root; **Landlock read/code rules** |
| `tmp_dir` | string | `/tmp/nusa` | Writable enclave; **Landlock write rules** |
| `hot_reload` | bool | `true` | Watch config file and atomically swap in-process |
| `octane_workers` | usize | `0` | `0` = Normal only; `> 0` = size of persistent worker pool |
| `octane_max_memory_mb` | u64 | `512` | Recycle worker after resident memory threshold |
| `octane_max_requests` | u64 | `1000` | Recycle worker after request count (leak containment) |

---

## Environment variables

Prefix: **`NUSA_`**. Figment maps to snake_case struct fields.

| Variable | Field | Example use |
|----------|-------|-------------|
| `NUSA_ENGINE` | `engine` | `child` in K8s manifest |
| `NUSA_MAX_WORKERS` | `max_workers` | Scale concurrency per deployment |
| `NUSA_TIMEOUT_MS` | `timeout_ms` | Tighten API route budget |
| `NUSA_OCTANE_WORKERS` | `octane_workers` | Enable Octane tier in staging |
| `NUSA_CODE_DIR` | `code_dir` | Mount Laravel at `/app` |
| `NUSA_TMP_DIR` | `tmp_dir` | Ephemeral volume for uploads |

This pattern keeps **12-factor** deployments straightforward: image holds code; ConfigMap/Secret holds runtime policy.

---

## Engine selection (deep dive)

### `child` — the default production path

Spawns governed PHP child processes per Normal-mode request semantics. You get:

- Familiar isolation compared to php-fpm pools  
- Clear failure domains when a worker misbehaves  
- No requirement to compile PHP with ZTS embed flags  

**Use when:** migrating from FPM, running multi-tenant pilots, or any deployment where operational simplicity beats embedded PHP.

### `ffi` — embedded PHP for specialized Linux images

Links against PHP built with embed/ZTS expectations. Lowest process overhead when your image pipeline already produces compatible libphp artifacts.

**Use when:** you control the entire PHP build chain and have validated FFI on target musl/glibc images.

### `wasm` — forward-looking sandbox (not production PHP today)

The WASM engine path explores **stronger isolation** via wasmtime limits (memory, fuel). In v0.1.0 the CLI still attaches a **stub** engine for `wasm`—do not enable in production expecting Laravel to run.

**Use when:** contributing to sandbox research, not serving customer traffic.

---

## Normal vs Octane topology

### Normal mode (`octane_workers = 0`)

Every HTTP request that reaches the PHP handler uses:

```
Gateway → PhpEngine::execute(RequestContext) → PHP → response
```

Backpressure and timeouts apply at the gateway; PHP lifecycle is request-scoped.

### Octane mode (`octane_workers > 0`)

The CLI constructs a **`WorkerPool`** sized to `octane_workers`, performs IPC handshake with each worker, and maintains recycle policy via memory and request counters.

**v0.1.0 behavior:**

```
Gateway → WorkerPool::handle_http_request → IPC → Laravel worker → response
```

When the pool is ready, HTTP does **not** call `engine.execute`. If the pool is configured but not ready, the gateway returns **503** (fail-closed).

Laravel setup: [Octane mode](laravel/octane-mode.md).

---

## Security-related paths (read twice)

`code_dir` and `tmp_dir` are not merely documentation—they are passed to **Landlock** before `axum` listens. If Laravel writes caches outside `tmp_dir`, or reads storage outside allowed rules, you will see permission failures that look like application bugs but are **policy doing its job**.

Align:

- `storage/` and `bootstrap/cache/` with writable policy  
- `public/` with `vfs_root` / `code_dir` layout you intend  
- Container volumes with orchestrator paths in config  

---

## Hot reload

With `hot_reload = true`, editing `nusa.toml` triggers an atomic config swap. Ideal for:

- Raising `max_workers` during a marketing event  
- Tightening `timeout_ms` when upstream latency spikes  
- Toggling `octane_workers` in controlled maintenance windows (once HTTP dispatch respects the pool)

You still need orchestrator-level rollouts for binary upgrades—config reload does not replace image updates.

---

## Example: conservative production pilot

```toml
engine = "child"
max_workers = 8
timeout_ms = 30000
code_dir = "/app"
vfs_root = "/app/public"
tmp_dir = "/tmp/nusa"
hot_reload = true
octane_workers = 0   # until P1 HTTP dispatch is verified in your environment
```

## Example: Octane staging (with eyes open)

```toml
engine = "child"
octane_workers = 4
octane_max_memory_mb = 512
octane_max_requests = 1000
code_dir = "/app"
tmp_dir = "/tmp/nusa"
```

Pair with [Production status](production-status.md) and [Migration](migration.md) before promoting to production traffic.

---

## Related documents

- [Getting started](getting-started.md)  
- [Operations runbook](operations/runbook.md)  
- [PHP ecosystem](ecosystem/package-guidelines.md)
