# Configuration reference

Nusa is configured through `nusa.toml` plus environment overrides. Config loads in this order: built-in defaults → TOML file → `NUSA_*` env vars (env wins on conflict). With `hot_reload = true`, the config swaps atomically at runtime.

- Implementation: [`crates/nusa-config/src/lib.rs`](../../crates/nusa-config/src/lib.rs)
- Example: [`config.toml.example`](../../config.toml.example)
- Container (env-only): [Laravel on Docker](laravel/docker.md)

---

## Key concepts

Three decisions drive most config choices:

1. **How PHP runs** — `engine`, `octane_workers`, recycle limits
2. **How the gateway protects itself** — `max_workers`, `timeout_ms`
3. **Where the kernel allows filesystem access** — `code_dir`, `tmp_dir`, `vfs_root`

---

## Key reference

### Top-level keys

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `engine` | `child` \| `ffi` \| `wasm` | `child` | PHP execution engine in Normal mode |
| `max_workers` | usize | `4` | Gateway concurrency ceiling (must be > 0) |
| `timeout_ms` | u64 | `30000` | Per-request upstream timeout (ms) |
| `wasm_memory_mb` | u64 | `256` | WASM sandbox limit when `engine = "wasm"` |
| `vfs_root` | string | *(empty)* | Web root; auto-resolves to `{code_dir}/public` |
| `code_dir` | string | `/app` | Laravel root where `artisan` lives |
| `tmp_dir` | string | `/tmp/nusa` | Writable sandbox for cache, sessions, uploads |
| `hot_reload` | bool | `true` | Watch and atomically swap config at runtime |
| `php_binary` | string | `php` | PHP executable for `engine = "child"` |
| `php_bootstrap` | string | *(empty)* | Custom bootstrap script for child IPC |
| `octane_workers` | usize | `0` | `0` = Normal only; `> 0` = persistent worker pool |
| `octane_max_memory_mb` | u64 | `512` | Recycle worker after this RSS (MB) |
| `octane_max_requests` | u64 | `1000` | Recycle worker after this request count |
| `octane_backend` | `ipc` \| `embed` | `ipc` | Transport for Octane workers |
| `octane_standby_workers` | usize | `0` | Warm standby workers for P99 recycle |
| `bind` | string | `0.0.0.0:8080` | HTTP listen address |
| `static_root` | string | *(empty)* | Static files root; auto-resolves to `{code_dir}/public` |
| `static_cache_max_age_secs` | u64 | `3600` | Cache-Control max-age for static assets |
| `static_cache_immutable_max_age_secs` | u64 | `31536000` | max-age for immutable assets (`.js`, `.css`, images) |
| `static_cache_max_entries` | usize | `1000` | LRU cache size limit |
| `static_cache_ttl_secs` | u64 | `300` | LRU cache entry TTL |

### Experimental sections

| Section | Key | Description |
|---------|-----|-------------|
| `[tls]` | `enabled` | ACME/TLS service (experimental; use reverse proxy in production) |
| `[tls]` | `acme_email` | Contact email for ACME |
| `[redis]` | `broadcast_url` | Redis pub/sub bridge for WS/SSE; empty = disabled |
| `[quic]` | `enabled` | Experimental QUIC/HTTP/3 (self-signed TLS; not production-ready) |
| `[quic]` | `bind` | QUIC listen address (default `0.0.0.0:443`) |

---

## Config file resolution

Resolution order (first match wins):

1. CLI `--config` path (file must exist)
2. `NUSA_CONFIG` env var (file must exist)
3. `./nusa.toml` in working directory
4. `/etc/nusa/nusa.toml`
5. Env-only — built-in defaults + `NUSA_*` (typical for containers)

Generate a starter: `nusa init` or `nusa init --octane` from the Laravel project root.

---

## Environment variables

Prefix: **`NUSA_`**. Values override the TOML file and built-in defaults.

### Top-level

| Variable | TOML key | Default |
|----------|----------|---------|
| `NUSA_CONFIG` | — | — |
| `NUSA_ENGINE` | `engine` | `child` |
| `NUSA_MAX_WORKERS` | `max_workers` | `4` |
| `NUSA_TIMEOUT_MS` | `timeout_ms` | `30000` |
| `NUSA_WASM_MEMORY_MB` | `wasm_memory_mb` | `256` |
| `NUSA_VFS_ROOT` | `vfs_root` | *(empty)* |
| `NUSA_CODE_DIR` | `code_dir` | `/app` |
| `NUSA_TMP_DIR` | `tmp_dir` | `/tmp/nusa` |
| `NUSA_HOT_RELOAD` | `hot_reload` | `true` |
| `NUSA_OCTANE_WORKERS` | `octane_workers` | `0` |
| `NUSA_OCTANE_MAX_MEMORY_MB` | `octane_max_memory_mb` | `512` |
| `NUSA_OCTANE_MAX_REQUESTS` | `octane_max_requests` | `1000` |
| `NUSA_OCTANE_BACKEND` | `octane_backend` | `ipc` |
| `NUSA_OCTANE_STANDBY_WORKERS` | `octane_standby_workers` | `0` |
| `NUSA_BIND` | `bind` | `0.0.0.0:8080` |
| `NUSA_STATIC_ROOT` | `static_root` | *(empty)* |
| `NUSA_PHP_BINARY` | `php_binary` | `php` |
| `NUSA_PHP_BOOTSTRAP` | `php_bootstrap` | *(empty)* |

### Nested variables (double underscore)

| Variable | TOML equivalent |
|----------|-----------------|
| `NUSA_TLS__ENABLED` | `[tls] enabled` |
| `NUSA_TLS__ACME_EMAIL` | `[tls] acme_email` |
| `NUSA_REDIS__BROADCAST_URL` | `[redis] broadcast_url` |
| `NUSA_QUIC__ENABLED` | `[quic] enabled` |
| `NUSA_QUIC__BIND` | `[quic] bind` |

**Note:** Use double underscore (`__`) for nested keys. `NUSA_TLS_ENABLED` (single underscore) will not map to `[tls]`.

### Laravel vs Nusa env

| Layer | Examples | Where |
|-------|----------|-------|
| Laravel | `APP_KEY`, `DB_*`, `SESSION_DRIVER` | `.env` / PHP workers |
| Nusa | `NUSA_*` | Container / orchestrator |

`NUSA_APP_KEY` has no effect — keep Laravel secrets in Laravel `.env`.

### Container example

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

---

## Multi-tenant registry

Optional `[[tenants]]` table in `nusa.toml`. When empty, any `X-Tenant-Id` is allowed (rate limits still apply). When non-empty, only listed tenants are accepted.

```toml
[[tenants]]
id = "acme"
vfs_root = "/var/tenants/acme/public"
enabled = true
max_memory_mb = 512
max_requests_per_minute = 1000
```

---

## Engine selection

### `child` (default)

Spawns governed PHP child processes per Normal-mode request.

**Use when:** migrating from FPM, running multi-tenant pilots, or any deployment where operational simplicity beats embedded PHP.

### `ffi`

Links against libphp built with embed/ZTS. Lowest process overhead.

**Use when:** you control the entire PHP build chain and have validated FFI on target musl/glibc images.

### `wasm` (not production-ready)

Explores stronger isolation via wasmtime limits (memory, fuel). In v0.1.0 the CLI attaches a stub engine — do not use for customer traffic.

---

## Execution modes

### Normal mode (`octane_workers = 0`)

```
Gateway → PhpEngine::execute → PHP → response
```

Each request uses a fresh PHP process. Backpressure and timeouts apply at the gateway.

### Octane mode (`octane_workers > 0`)

```
Gateway → WorkerPool::handle_http_request → transport → Laravel worker → response
```

When the pool is ready, HTTP dispatch goes to workers (not `engine.execute`). If the pool is configured but not ready, the gateway returns **503** (fail-closed).

Setup: [Octane mode](laravel/octane-mode.md).

---

## Security-related paths

`code_dir` and `tmp_dir` are passed to **Landlock** before `axum` listens. If Laravel writes caches outside `tmp_dir`, or reads storage outside allowed rules, you will see permission failures.

Align:
- `storage/` and `bootstrap/cache/` with writable policy
- `public/` with `vfs_root` / `code_dir` layout
- Container volumes with orchestrator paths

---

## Example configurations

### Normal mode (conservative pilot)

```toml
engine = "child"
max_workers = 8
timeout_ms = 30000
code_dir = "/app"
vfs_root = "/app/public"
tmp_dir = "/tmp/nusa"
hot_reload = true
octane_workers = 0
```

### Octane mode (staging)

```toml
engine = "child"
octane_workers = 4
octane_max_memory_mb = 512
octane_max_requests = 1000
code_dir = "/app"
tmp_dir = "/tmp/nusa"
```

---

## Related documents

- [Laravel configuration](laravel/configuration-for-laravel.md)
- [Laravel on Docker](laravel/docker.md)
- [Operations runbook](operations/runbook.md)
