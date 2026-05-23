# Nusa documentation for Laravel developers

**Nusa** runs your Laravel application behind a single `nusa` process: HTTP, health checks, metrics, rate limits, and (optionally) long-lived **Octane-style workers** — without maintaining nginx + php-fpm + Supervisor as separate products.

You keep writing **routes, controllers, Eloquent, and Composer packages** in PHP. Nusa handles the platform edge in Rust.

> **Audience:** Laravel developers and tech leads shipping Laravel to production.
> **Rust contributors:** see [contributor documentation](../contributor/README.md).

**Release:** v0.1.0 (pre-GA). Read [Production status](production-status.md) before production cutover.

---

## Start here

| I want to… | Read this | Time |
|------------|-----------|------|
| Run Nusa locally | [Quick start](quick-start.md) | ~10 min |
| Understand install + layout | [Installation](laravel/installation.md) | ~20 min |
| Use Octane / persistent workers | [Octane mode](laravel/octane-mode.md) | ~25 min |
| Tune `nusa.toml` for my app | [Configuration](configuration.md) | reference |
| Move from FPM or RoadRunner | [Migration](migration.md) | ~30 min |
| Fix errors / 503 / permissions | [Troubleshooting](laravel/troubleshooting.md) | when needed |

---

## Laravel documentation

Structured guides for day-to-day Laravel work:

| Guide | Topics |
|-------|--------|
| [Installation](laravel/installation.md) | PHP, Composer, `nusa` binary, paths |
| [Run your existing app](laravel/first-app.md) | `code_dir`, first request, probes |
| [Configuration for Laravel](laravel/configuration-for-laravel.md) | `code_dir`, `storage/`, Octane knobs |
| [Normal mode (FPM-like)](laravel/normal-mode.md) | `octane_workers = 0`, when to use it |
| [Octane mode](laravel/octane-mode.md) | Workers, driver, IPC, readiness |
| [PHP driver package](laravel/php-driver.md) | `nusa/octane`, `NusaOctaneServiceProvider`, `nusa-octane-worker` |
| [HTTP, routes, headers](laravel/routes-http-and-headers.md) | GET/POST, query strings, tracing |
| [Local development](laravel/local-development.md) | `nusa dev`, hot reload |
| [Deploy checklist](laravel/deployment-checklist.md) | Staging → production |
| [Troubleshooting](laravel/troubleshooting.md) | Common Laravel + Nusa issues |
| [FAQ](laravel/faq.md) | Short answers |

---

## Platform reference

| Document | Purpose |
|----------|---------|
| [Configuration reference](configuration.md) | Every `nusa.toml` key |
| [Production status](production-status.md) | What is production-grade today |
| [Compatibility matrix](compatibility-matrix.md) | PHP, Laravel, OS versions |
| [Migration](migration.md) | From FPM, RoadRunner, FrankenPHP |
| [Operations runbook](operations/runbook.md) | SRE: incidents, scaling, probes |
| [PHP ecosystem](ecosystem/package-guidelines.md) | Package layout (contributor-oriented) |

---

## How Nusa fits your Laravel app

```
  Browser / API client
           │
           ▼
  ┌──────────────────────────┐
  │  nusa (gateway)          │
  │  /health  /ready /metrics│
  └────────────┬─────────────┘
               │
      ┌────────┴────────┐
      ▼                 ▼
  Normal mode        Octane mode
  (workers = 0)      (workers > 0)
  One PHP/request    Laravel workers
```

| Mode | Config | Feels like |
|------|--------|------------|
| **Normal** | `octane_workers = 0` | php-fpm: fresh request scope |
| **Octane** | `octane_workers = 4` | Laravel Octane / RoadRunner: warm workers |

---

## Security and compliance

- [Threat model](../security/threat-model.md)
- [Compliance](../security/compliance.md)
- [SECURITY.md](../../SECURITY.md)
