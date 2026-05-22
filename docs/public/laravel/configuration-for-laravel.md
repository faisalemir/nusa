# Configuration for Laravel

Laravel-specific guidance for `nusa.toml`. For the full key list see [Configuration reference](../configuration.md).

---

## The three paths that matter

| Key | Laravel meaning | Example |
|-----|-----------------|---------|
| **`code_dir`** | Project root (`artisan`, `app/`, `config/`, `vendor/`) | `/app` in Kubernetes |
| **`vfs_root`** | Web root (`public/index.php`) | `/app/public` |
| **`tmp_dir`** | Writable enclave for Nusa sandbox policy | `/tmp/nusa` or mounted volume |

If uploads, sessions, or `storage/logs` fail with **permission denied**, the fix is almost always path alignment—not disabling security.

---

## Recommended starter `nusa.toml`

### Local development (Normal mode)

```toml
engine = "child"
octane_workers = 0
code_dir = "/home/you/myapp"
vfs_root = "/home/you/myapp/public"
tmp_dir  = "/tmp/nusa-myapp"
max_workers = 4
timeout_ms = 60000
hot_reload = true
```

### Staging / production (Octane)

```toml
engine = "child"
octane_workers = 4
octane_max_memory_mb = 512
octane_max_requests = 1000
code_dir = "/app"
vfs_root = "/app/public"
tmp_dir  = "/var/nusa/tmp"
max_workers = 32
timeout_ms = 30000
hot_reload = false
```

Scale `octane_workers` with CPU cores and memory—measure p95 latency under load.

---

## Environment variables (12-factor)

Override TOML without editing files in the image:

| Variable | Laravel use case |
|----------|------------------|
| `NUSA_CODE_DIR` | Helm chart sets mount path |
| `NUSA_TMP_DIR` | Ephemeral volume for cache/sessions |
| `NUSA_OCTANE_WORKERS` | Different worker count per environment |
| `NUSA_MAX_WORKERS` | Gateway concurrency ceiling |
| `NUSA_TIMEOUT_MS` | API route budget |

`.env` remains for **Laravel** (`APP_KEY`, database, etc.). **`NUSA_*`** is for the **runtime**.

---

## `storage/` and `bootstrap/cache/`

Laravel expects to write:

- `storage/framework/cache`  
- `storage/framework/sessions`  
- `storage/framework/views`  
- `storage/logs`  
- `bootstrap/cache`  

**Options:**

1. Ensure `tmp_dir` and Landlock policy allow writes to mounted `storage/` (production pattern).  
2. Use `SESSION_DRIVER=array` or `redis` in `.env` so file sessions are not required in restricted sandboxes.  
3. Point `FILESYSTEM_DISK` to S3 for uploads in multi-instance deployments.  

Session/middleware E2E coverage is still expanding—test your stack in staging.

---

## Octane-related keys

| Key | What Laravel developers should know |
|-----|-------------------------------------|
| `octane_workers` | `0` = off; `> 0` = persistent workers (like Octane) |
| `octane_max_memory_mb` | Recycle worker after RSS threshold (leak safety) |
| `octane_max_requests` | Recycle after N requests (leak safety) |

When `octane_workers > 0`:

- Nusa **exits on startup** if the pool cannot initialize or has no IPC transport.  
- **`/ready`** returns 503 until the pool is ready.  

---

## Engines (`engine` key)

| Value | Use for Laravel |
|-------|-----------------|
| **`child`** | **Default** — works everywhere PHP CLI works |
| **`ffi`** | Advanced Linux only — embedded PHP |
| **`wasm`** | **Not** for Laravel today — research only |

---

## Hot reload

`hot_reload = true` reloads **`nusa.toml`** when the file changes (not every `.env` change).

For Laravel code changes while developing, use:

```bash
nusa dev --pretty
```

→ [Local development](local-development.md)

---

## Next steps

- [Normal mode](normal-mode.md)  
- [Octane mode](octane-mode.md)  
- [Configuration reference](../configuration.md)  
