# Configuration reference

Nusa is operated through a **small, explicit configuration surface**—`nusa.toml` plus environment overrides—because platform teams should not reverse-engineer PHP ini files to understand concurrency, sandbox paths, or worker topology.

Configuration loads through **figment**: **built-in defaults → TOML file (if present) → `NUSA_*` environment variables** (`NUSA_*` wins on conflict). Hot reload swaps the active config with **`ArcSwap`** when `hot_reload = true`; invalid TOML on reload is rejected and the last good snapshot is kept.

Implementation source: [`crates/nusa-config/src/lib.rs`](../../crates/nusa-config/src/lib.rs).  
Example file: [`config.toml.example`](../../config.toml.example).  
Laravel containers (env-only, no `nusa.toml`): [Laravel on Docker](laravel/docker.md).

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
| `vfs_root` | string | *(empty)* | Web root; empty → `{code_dir}/public` after path normalization |
| `code_dir` | string | `/app` | Laravel root (`artisan`); **Landlock read/code rules** — not `public/` alone |
| `tmp_dir` | string | `/tmp/nusa` | Writable enclave; **Landlock write rules** |
| `hot_reload` | bool | `true` | Watch config file and atomically swap in-process |
| `php_binary` | string | `php` | PHP executable when `engine = "child"` |
| `php_bootstrap` | string | *(empty)* | Child IPC bootstrap script; empty uses default bootstrap |
| `octane_workers` | usize | `0` | `0` = Normal only; `> 0` = size of persistent worker pool |
| `octane_max_memory_mb` | u64 | `512` | Recycle worker after resident memory threshold |
| `octane_max_requests` | u64 | `1000` | Recycle worker after request count (leak containment) |
| `bind` | string | `0.0.0.0:8080` | HTTP listen address |
| `static_root` | string | *(empty)* | Static files root; empty → `{code_dir}/public` |

### Experimental (default off)

| Section / key | Purpose |
|---------------|---------|
| `[tls] enabled` | ACME/TLS service spawn (experimental; use reverse proxy for production) |
| `[tls] acme_email` | Contact email when TLS is enabled |
| `[redis] broadcast_url` | Redis pub/sub bridge for WS/SSE fan-out; empty = disabled |
| `[quic] enabled` | Spawns experimental UDP QUIC accept loop (self-signed TLS; HTTP/3 not production-ready) |
| `[quic] bind` | QUIC listen address (default `0.0.0.0:443`) |

---

## How the config file is found

Resolution order (first match wins):

| Step | Source |
|------|--------|
| 1 | CLI `--config` path (if the file exists) |
| 2 | `NUSA_CONFIG` (must point to an existing file) |
| 3 | `./nusa.toml` in the working directory |
| 4 | `/etc/nusa/nusa.toml` |
| 5 | **Env-only** — no file; built-in defaults + `NUSA_*` (typical for containers) |

Generate a starter file locally: `nusa init` or `nusa init --octane` from the Laravel project root.

---

## Environment variables (`NUSA_*`)

Prefix: **`NUSA_`**. Values override the TOML file and built-in defaults. Use **double underscore** (`__`) only for nested TOML tables (not for flat keys like `max_workers`).

### Top-level variables

| Variable | TOML key | Default | Notes |
|----------|----------|---------|--------|
| `NUSA_CONFIG` | — | — | Path to `nusa.toml` (file must exist) |
| `NUSA_ENGINE` | `engine` | `child` | `child`, `ffi`, or `wasm` |
| `NUSA_MAX_WORKERS` | `max_workers` | `4` | Gateway concurrency; must be > 0 |
| `NUSA_TIMEOUT_MS` | `timeout_ms` | `30000` | Per-request timeout (ms) |
| `NUSA_WASM_MEMORY_MB` | `wasm_memory_mb` | `256` | WASM sandbox limit |
| `NUSA_VFS_ROOT` | `vfs_root` | *(empty)* | Web root; auto `{code_dir}/public` when empty |
| `NUSA_CODE_DIR` | `code_dir` | `/app` | Laravel root (`artisan`) |
| `NUSA_TMP_DIR` | `tmp_dir` | `/tmp/nusa` | Writable sandbox; mount a volume in K8s |
| `NUSA_HOT_RELOAD` | `hot_reload` | `true` | Set `false` in production images |
| `NUSA_OCTANE_WORKERS` | `octane_workers` | `0` | `> 0` enables Octane worker pool |
| `NUSA_OCTANE_MAX_MEMORY_MB` | `octane_max_memory_mb` | `512` | Recycle worker after memory (MB) |
| `NUSA_OCTANE_MAX_REQUESTS` | `octane_max_requests` | `1000` | Recycle worker after request count |
| `NUSA_BIND` | `bind` | `0.0.0.0:8080` | HTTP listen address |
| `NUSA_STATIC_ROOT` | `static_root` | *(empty)* | Static files; empty → `{code_dir}/public` |
| `NUSA_PHP_BINARY` | `php_binary` | `php` | PHP binary for `engine = "child"` |
| `NUSA_PHP_BOOTSTRAP` | `php_bootstrap` | *(empty)* | Child IPC bootstrap script path |

Landlock also grants **RW** to `{code_dir}/storage` and `{code_dir}/bootstrap/cache` when those directories exist (file session/cache). Prefer **redis** for multi-replica.

### Nested variables (`NUSA_<SECTION>__<KEY>`)

| Variable | TOML equivalent |
|----------|-----------------|
| `NUSA_TLS__ENABLED` | `[tls] enabled` |
| `NUSA_TLS__ACME_EMAIL` | `[tls] acme_email` |
| `NUSA_REDIS__BROADCAST_URL` | `[redis] broadcast_url` |
| `NUSA_QUIC__ENABLED` | `[quic] enabled` |
| `NUSA_QUIC__BIND` | `[quic] bind` |

Do **not** use `NUSA_TLS_ENABLED` (single underscore) for nested keys — Figment will not map it to `[tls]`.

### Laravel vs Nusa env

| Layer | Examples | Where |
|-------|----------|--------|
| Laravel | `APP_KEY`, `DB_*`, `SESSION_DRIVER` | `.env` / PHP workers |
| Nusa | `NUSA_*` | Container / orchestrator |

`NUSA_APP_KEY` has no effect; keep Laravel secrets in Laravel `.env`.

### Container example (Octane, env-only)

```yaml
environment:
  NUSA_CODE_DIR: /app
  NUSA_TMP_DIR: /tmp/nusa
  NUSA_OCTANE_WORKERS: "4"
  NUSA_MAX_WORKERS: "32"
  NUSA_TIMEOUT_MS: "30000"
  NUSA_BIND: "0.0.0.0:8080"
  NUSA_HOT_RELOAD: "false"
```

More detail: [Laravel on Docker](laravel/docker.md), [`docker-compose.laravel.example.yml`](../../docker-compose.laravel.example.yml).

### Multi-tenant registry (`[[tenants]]`)

Optional table in `nusa.toml`. When **empty**, any `X-Tenant-Id` / host subdomain is allowed (rate limits still apply). When **non-empty**, only listed tenant IDs are accepted.

```toml
[[tenants]]
id = "acme"
vfs_root = "/var/tenants/acme/public"
enabled = true
max_memory_mb = 512
max_requests_per_minute = 1000
```

HTTP `Cookie` headers are parsed into the IPC `cookies` field for Octane workers and child IPC (Laravel `Request::create`).

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
octane_workers = 0   # Normal mode; set > 0 for Octane after driver + staging validation
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
- [Laravel on Docker](laravel/docker.md)  
- [Configuration for Laravel](laravel/configuration-for-laravel.md)  
- [Operations runbook](operations/runbook.md)  
- [PHP ecosystem](ecosystem/package-guidelines.md)
