# Laravel on Docker / Kubernetes (plug-and-play)

Run Nusa **without maintaining `nusa.toml` in the image** — use environment variables only (12-factor). Paths default to a standard Laravel layout: app at `/app`, `public/` as web root.

---

## Minimal container env

| Variable | Required | Default | Purpose |
|----------|----------|---------|---------|
| `NUSA_CODE_DIR` | Recommended | `/app` | Laravel root (`artisan` lives here) |
| `NUSA_VFS_ROOT` | Optional | `{code_dir}/public` | Web root |
| `NUSA_TMP_DIR` | Recommended | `/tmp/nusa` | Writable sandbox (mount a volume) |
| `NUSA_BIND` | Optional | `0.0.0.0:8080` | HTTP listen |
| `NUSA_OCTANE_WORKERS` | For Octane | `4` | `> 0` enables worker pool |
| `NUSA_OCTANE_BACKEND` | Optional | `ipc` | `ipc` (PHP subprocess) or `embed` (in-process / stdio daemon) |
| `NUSA_OCTANE_STANDBY_WORKERS` | Optional | `1` in container defaults | Pre-bootstrapped workers for fast recycle |
| `NUSA_MAX_WORKERS` | Optional | `4` | Gateway concurrency |
| `NUSA_TIMEOUT_MS` | Optional | `30000` | Request timeout |
| `NUSA_ENGINE` | Optional | `child` | `child` \| `ffi` \| `wasm` |
| `NUSA_HOT_RELOAD` | Optional | `false` (env-only) | `true` in dev |
| `NUSA_CONFIG` | Optional | — | Path to TOML if you still use a file |

**No `nusa.toml` required** if all policy is in env. Startup logs: `No nusa.toml found; using NUSA_* ...`.

### Production Octane example

```yaml
environment:
  NUSA_CODE_DIR: /app
  NUSA_TMP_DIR: /tmp/nusa
  NUSA_OCTANE_WORKERS: "4"
  NUSA_OCTANE_BACKEND: "ipc"
  NUSA_MAX_WORKERS: "32"
  NUSA_TIMEOUT_MS: "30000"
  NUSA_BIND: "0.0.0.0:8080"
  NUSA_HOT_RELOAD: "false"
```

Mount Laravel code at `/app`, mount a writable volume at `/tmp/nusa`, and ensure `vendor/` exists in the image or an init container.

---

## Nested settings via env

Figment maps `NUSA_` + `__` for nested TOML:

| Env | TOML equivalent |
|-----|-----------------|
| `NUSA_TLS__ENABLED=true` | `[tls] enabled = true` |
| `NUSA_REDIS__BROADCAST_URL=redis://redis:6379` | `[redis] broadcast_url` |
| `NUSA_QUIC__ENABLED=false` | `[quic] enabled` |

---

## Docker Compose sketch

See [`docker-compose.laravel.example.yml`](../../../docker-compose.laravel.example.yml) in the repo root.

---

## Laravel `.env` vs Nusa env

| Layer | Examples |
|-------|----------|
| **Laravel** (`APP_*`, `DB_*`, `SESSION_DRIVER`) | Inside PHP workers — unchanged |
| **Nusa** (`NUSA_*`) | Process limits, paths, Octane pool size, bind address |

Do not mix them: `NUSA_APP_KEY` does nothing; use `APP_KEY` in Laravel `.env`.

---

## Sessions and cache in containers

| Driver | Recommendation |
|--------|----------------|
| `SESSION_DRIVER` | `redis` or `database` for multi-replica |
| `CACHE_STORE` | `redis` |
| File drivers | Only with shared storage + Landlock-aligned paths |

See [Configuration for Laravel](configuration-for-laravel.md).

---

## Local file generation

From your Laravel project root:

```bash
nusa init              # Normal mode nusa.toml
nusa init --octane     # Octane starter (4 workers)
```

---

## Next steps

- [Installation](installation.md)
- [Configuration reference](../configuration.md) (full `NUSA_*` table)
- [Configuration for Laravel](configuration-for-laravel.md)
- [Octane mode](octane-mode.md)
