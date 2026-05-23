# Nusa — run Laravel on a modern PHP runtime

**One process for HTTP, health checks, metrics, and (optionally) Octane-style workers.** You keep Laravel, Composer, and your routes. Nusa replaces the usual **nginx + php-fpm + Supervisor** stack with a single `nusa` binary.

[![Version](https://img.shields.io/badge/version-0.1.0-blue)](docs/public/production-status.md)
[![License](https://img.shields.io/badge/license-MIT-green)](#license)

> **Release 0.1.0 (pre-GA)** — ready for staging and pilots on **Alpine Linux**. Read [production status](docs/public/production-status.md) before production cutover.

**New here?** → [Quick start (10 min)](docs/public/quick-start.md) · [Full Laravel guide](docs/public/laravel/README.md)

---

## Why Laravel teams use Nusa

| Instead of… | You get… |
|-------------|----------|
| nginx + php-fpm pool files | One `nusa` process with `max_workers` and timeouts in `nusa.toml` |
| RoadRunner or FrankenPHP as a separate server | Optional **Octane mode** in the same binary (`octane_workers > 0`) |
| Security only in `php.ini` | **Landlock + seccomp** on Linux before the server accepts traffic |
| Bolted-on metrics and probes | Built-in `/health`, `/ready`, `/metrics`, tracing headers |
| Scattered config | **`nusa.toml`** + `NUSA_*` env vars (Kubernetes-friendly) |

You still run **`php artisan`**, **Composer**, **Eloquent**, and your **`routes/`** — only the platform layer changes.

---

## Two ways to run your app

| Mode | Config | Feels like | Best for |
|------|--------|------------|----------|
| **Normal** | `octane_workers = 0` | php-fpm — fresh request each time | First install, migrations from FPM, max isolation |
| **Octane** | `octane_workers = 4` (example) | Laravel Octane / RoadRunner — warm workers | Throughput, APIs under load |

```text
  Browser
     │
     ▼
  nusa (HTTP + /health /ready /metrics)
     │
     ├─ Normal  → PHP per request
     └─ Octane  → Laravel workers (`nusa/octane`, `nusa-octane-worker`) over IPC
```

Octane mode **fails closed**: if workers cannot start, `nusa` exits and `/ready` returns 503 until the pool is healthy.

Guides: [Normal mode](docs/public/laravel/normal-mode.md) · [Octane mode](docs/public/laravel/octane-mode.md)

---

## Quick start

**Requirements:** PHP 8.2+, Composer, Rust 1.95+ (to build `nusa` from source).

```bash
git clone https://github.com/nusa-rs/nusa.git
cd nusa

cp config.toml.example nusa.toml
```

Point `nusa.toml` at your Laravel project (where `artisan` lives):

```toml
engine = "child"
octane_workers = 0

code_dir = "/path/to/my-laravel-app"
vfs_root = "/path/to/my-laravel-app/public"
tmp_dir  = "/tmp/nusa-myapp"
```

```bash
cargo build -p nusa-cli --release
./target/release/nusa --config nusa.toml
```

```bash
curl http://127.0.0.1:8080/health
curl http://127.0.0.1:8080/ready
curl http://127.0.0.1:8080/
```

**Daily dev** (watch PHP files):

```bash
cargo run -p nusa-cli -- dev --pretty
```

**Enable Octane:** install [nusa/octane](docs/public/laravel/php-driver.md) (`php-driver/`), set `octane_workers > 0`, restart. Details: [Octane mode](docs/public/laravel/octane-mode.md).

---

## Configuration at a glance

| Key | What it means for Laravel |
|-----|---------------------------|
| `code_dir` | Project root (`artisan`, `app/`, `vendor/`) |
| `vfs_root` | Usually `public/` |
| `tmp_dir` | Writable path for cache/sessions (sandbox policy) |
| `max_workers` | How many concurrent requests the gateway accepts |
| `timeout_ms` | Max wait time per request |
| `octane_workers` | `0` = off; `> 0` = persistent workers |
| `octane_max_memory_mb` / `octane_max_requests` | When to restart a worker (leak safety) |

Override with env vars: `NUSA_CODE_DIR`, `NUSA_OCTANE_WORKERS`, etc.

Full reference: [Configuration](docs/public/configuration.md) · Laravel-focused: [configuration for Laravel](docs/public/laravel/configuration-for-laravel.md)

---

## What works in v0.1.0

- HTTP to Laravel in **Normal mode** (`engine = "child"`)
- **Octane mode** — gateway sends traffic to the worker pool when it is ready (not the generic PHP engine path)
- **`/ready`** fails if Octane is enabled but workers are unhealthy
- **PHP driver** for long-lived Laravel workers
- Prometheus **`/metrics`**, structured logs, WebSocket and SSE routes
- Per-tenant rate limits and circuit breakers (platform apps)
- Alpine CI validation (`just podman-ci`)

**Before GA:** FPM comparison benchmarks on release hardware, your staging sign-off on Alpine (`just podman-ci-e2e`). See [production status](docs/public/production-status.md).

---

## Documentation (Laravel developers)

| I want to… | Read |
|------------|------|
| Install and first request | [Installation](docs/public/laravel/installation.md) |
| Run an existing app | [First app](docs/public/laravel/first-app.md) |
| Move from FPM or RoadRunner | [Migration](docs/public/migration.md) |
| Fix 503 / permissions / Octane | [Troubleshooting](docs/public/laravel/troubleshooting.md) |
| Deploy to production | [Deployment checklist](docs/public/laravel/deployment-checklist.md) |
| Short answers | [FAQ](docs/public/laravel/faq.md) |

**Hub:** [docs/public/laravel/README.md](docs/public/laravel/README.md) · **Index:** [docs/public/README.md](docs/public/README.md)

---

## Migrating?

| From | Start here |
|------|------------|
| **php-fpm + nginx** | [Normal mode](docs/public/laravel/normal-mode.md) + [Migration → FPM](docs/public/migration.md#from-php-fpm) |
| **RoadRunner / FrankenPHP** | [Octane mode](docs/public/laravel/octane-mode.md) + [PHP driver](docs/public/laravel/php-driver.md) |

---

## PHP driver (Composer)

For Octane mode, add the path package and run `composer install`:

```json
{
  "repositories": [{ "type": "path", "url": "../php-driver" }],
  "require": { "nusa/octane": "@dev" }
}
```

Worker binary: `nusa-octane-worker`. Step-by-step: [PHP driver package](docs/public/laravel/php-driver.md).

---

## Operations and security

| Topic | Document |
|-------|----------|
| SRE runbook (incidents, scaling) | [Operations runbook](docs/public/operations/runbook.md) |
| PHP / Laravel / OS versions | [Compatibility matrix](docs/public/compatibility-matrix.md) |
| Threat model | [docs/security/threat-model.md](docs/security/threat-model.md) |
| Report a vulnerability | [SECURITY.md](SECURITY.md) |

---

## Contributing to Nusa (Rust runtime)

This repository is the **runtime** (Rust gateway, workers, IPC, sandbox). Laravel apps live in your own repo.

| Task | Link |
|------|------|
| Build, test, `just` commands | [CONTRIBUTING.md](CONTRIBUTING.md) |
| Architecture and crates | [docs/contributor/](docs/contributor/) |
| Alpine CI gate | `just podman-ci` ([testing guide](docs/contributor/testing.md)) |

---

## License

MIT — see `Cargo.toml` in this repository.
