# Local development

Day-to-day Laravel development with Nusa: fast reload, readable logs, and the same `nusa.toml` you use in staging.

---

## Recommended workflow

1. **Normal mode** while building features (`octane_workers = 0`).  
2. Enable **Octane mode** when testing performance or Octane-specific bugs.  
3. Use **`nusa dev`** instead of manual restarts when editing PHP files.  

---

## `nusa dev`

Watches Laravel directories and reloads with debounced restarts:

```bash
cargo run -p nusa-cli -- dev --pretty
```

Typical watch targets: `app/`, `config/`, `routes/`, `resources/views/`, `.env`.

| Flag | Effect |
|------|--------|
| `--pretty` | Human-friendly log formatting |
| `--watch PATH` | Extra paths (repeatable) |
| `--debounce MS` | Debounce file events (default sensible) |

`dev` still reads **`nusa.toml`** (or `--config`).

---

## Hot reload vs Laravel reload

| Mechanism | What reloads |
|-----------|----------------|
| `hot_reload = true` in `nusa.toml` | **Nusa config file** only (`ArcSwap`) |
| `nusa dev` | **PHP/Laravel code** when files change |
| `php artisan octane:reload` | Not the primary model—use Nusa recycle or restart |

---

## `.env` and config cache

During local dev:

```env
APP_ENV=local
APP_DEBUG=true
```

Avoid `config:cache` while actively editing `config/*.php` unless you intentionally test production mode.

---

## Debugging Octane locally

1. Start with `octane_workers = 1` to simplify logs.  
2. Confirm driver: `php vendor/.../octane-rust-worker --help` or run via pool logs.  
3. Use fixture routes (`/nusa-ping`) if you cloned the Nusa repo with `tests/fixtures/laravel-minimal`.  

Linux or WSL gives the closest behavior to Alpine production.

---

## Host OS notes

| OS | Dev experience |
|----|----------------|
| **Linux / WSL** | Best match for sandbox + Octane |
| **macOS** | Build and run OK; treat Alpine CI as truth for prod |
| **Windows** | Build OK; use WSL for Octane IPC parity |

---

## IDE and Xdebug

Xdebug with long-lived Octane workers requires the same discipline as RoadRunner: attach to worker processes, expect restarts on recycle. Normal mode is simpler for step debugging.

---

## Next steps

- [Quick start](../quick-start.md)  
- [Troubleshooting](troubleshooting.md)  
