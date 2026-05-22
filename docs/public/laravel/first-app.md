# Run your existing Laravel app

Wire an existing Laravel project to Nusa and confirm HTTP, logs, and probes.

---

## Before you start

- [ ] `composer install` completed in the project root  
- [ ] `.env` exists (`cp .env.example .env`, `php artisan key:generate` if needed)  
- [ ] `nusa.toml` created ([Installation](installation.md))  
- [ ] `storage/` and `bootstrap/cache/` writable by the PHP user  

---

## 1. Set paths in `nusa.toml`

Example for a typical app at `~/apps/shop`:

```toml
code_dir = "/home/deploy/apps/shop"
vfs_root = "/home/deploy/apps/shop/public"
tmp_dir  = "/var/nusa/shop-tmp"
```

In Docker/Kubernetes, mount:

- Application code → `code_dir`  
- `public/` → can match `vfs_root`  
- A volume or emptyDir → `tmp_dir` **and** Laravel `storage/` if policy requires it  

See [Configuration for Laravel](configuration-for-laravel.md).

---

## 2. Start in Normal mode

```toml
octane_workers = 0
engine = "child"
```

```bash
nusa --config nusa.toml
```

---

## 3. Smoke-test routes

Replace with routes your app actually exposes:

```bash
# Probes (Nusa — not Laravel routes)
curl -s http://127.0.0.1:8080/health
curl -s http://127.0.0.1:8080/ready

# Application (via public/index.php semantics)
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:8080/
curl -s http://127.0.0.1:8080/api/health   # if you have an API route
```

| HTTP code | Meaning |
|-----------|---------|
| **200** | Request reached PHP/Laravel successfully |
| **502/504** | PHP timeout or upstream error — check logs |
| **503** | Gateway not ready (Octane pool unhealthy when workers enabled) |

---

## 4. Check logs

Nusa emits structured logs to stderr. Look for:

- Sandbox apply success on Linux  
- PHP fatal errors from your app  
- `octane_workers` initialization when using Octane  

Tune Laravel `APP_DEBUG` in `.env` only in non-production environments.

---

## 5. Enable Octane (optional)

When Normal mode is stable:

1. [Install PHP driver](php-driver.md)  
2. Set `octane_workers = 2` (start small)  
3. Restart Nusa  
4. `/ready` must return **200** before load balancers send traffic  

Details: [Octane mode](octane-mode.md).

---

## Fixture reference (CI)

The repository includes a minimal Laravel app used in automated tests:

`tests/fixtures/laravel-minimal/`

Routes useful for debugging IPC:

| Route | Response |
|-------|----------|
| `GET /` | `nusa-fixture-ok` |
| `GET /nusa-ping` | `pong` |
| `POST /nusa-echo` | echoes body |
| `GET /nusa-query?q=...` | reflects query string |
| `GET /nusa-counter` | increments per worker (state test) |

---

## Next steps

- [Local development](local-development.md)  
- [Deploy checklist](deployment-checklist.md)  
- [Troubleshooting](troubleshooting.md)  
