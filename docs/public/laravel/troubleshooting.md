# Troubleshooting (Laravel + Nusa)

Symptoms → likely cause → what to do.

---

## Startup fails immediately

| Message / symptom | Cause | Fix |
|-------------------|-------|-----|
| `octane_workers=... failed to initialize` | PHP binary or driver missing | Install [PHP driver](php-driver.md); check `PATH` in container |
| `no worker has IPC transport` | Workers did not handshake | Run `composer install`; verify `nusa-octane-worker` exists under `code_dir` |
| `engine=wasm` rejected | WASM not for Laravel | Use `engine = "child"` |
| Landlock apply error | Invalid `code_dir` / `tmp_dir` | Paths must exist; use absolute paths in containers |

---

## `/ready` returns 503

| Context | Cause | Fix |
|---------|-------|-----|
| Octane enabled | Pool not `is_ready()` | Fix worker spawn; check logs at startup |
| Octane disabled | Gateway health not marked ready | Check earlier startup errors |
| After deploy | Race: probe before workers up | Increase `initialDelaySeconds` on readiness probe |

---

## Permission denied (storage, uploads, logs)

| Cause | Fix |
|-------|-----|
| Laravel writes outside Landlock-allowed paths | Align `tmp_dir` and volume mounts with `storage/` and `bootstrap/cache/` |
| Wrong UID in container | Match PHP user to volume `fsGroup` / permissions |
| `storage` not created | `php artisan storage:link` + ensure directories exist in image |

See [Configuration for Laravel](configuration-for-laravel.md).

---

## 404 on all routes

| Cause | Fix |
|-------|-----|
| `vfs_root` not pointing to `public/` | Set `vfs_root = ".../public"` |
| Missing `public/index.php` | Standard Laravel layout required |
| Trailing slash / rewrite rules | Compare with nginx config you replaced |

---

## 502 / 504 / upstream errors

| Cause | Fix |
|-------|-----|
| `timeout_ms` too low | Increase for slow queries; move heavy work to queues |
| PHP fatal in Laravel | Fix app error; check `storage/logs/laravel.log` |
| Database unreachable from container | Network policy / `.env` `DB_HOST` |

---

## Octane mode

### Workers spawn but wrong response body

- Confirm route exists in Laravel.  
- Test fixture route pattern: plain text routes first, then full MVC.  

### State leaks between requests

- Globals/statics in Laravel code—refactor (same as RoadRunner).  
- Lower `octane_max_requests` to force recycle while debugging.  

### `nusa-counter` style tests fail

- Counter increments per **worker**—expected with multiple workers.  

---

## Works locally, fails in production

| Cause | Fix |
|-------|-----|
| macOS/Windows ≠ Alpine musl | Staging on Alpine; run `just podman-ci` |
| Different PHP extensions | Match `php85` extensions in CI Dockerfile |
| `.env` missing in image | Inject via secrets; never rely on local-only `.env` |

---

## Composer / driver

| Symptom | Fix |
|---------|-----|
| `vendor/nusa/octane` missing | `composer install`; path repository URL correct |
| Symlink `php-driver` broken in container | Use `ln -sfn` in entrypoint (see `dockerfiles/podman-laravel-fixture.sh`) |

---

## Still stuck?

1. Capture: `nusa` stderr logs, `curl -v` to `/health` and `/ready`, Laravel log.  
2. Reproduce on minimal fixture: `tests/fixtures/laravel-minimal`.  
3. Open an issue with OS, PHP version, `nusa.toml` (redact secrets), and log excerpts.  

Contributors: [Testing guide](../../contributor/testing.md).
