# Migration guide

Moving Laravel to Nusa is a platform change, not a find-and-replace. You gain a unified edge (HTTP, metrics, tenancy, circuit breaking, kernel sandbox) and retire nginx + php-fpm + Supervisor as separate products.

## What you are migrating toward

| Legacy pain | Nusa answer |
|-------------|-------------|
| nginx + php-fpm + Supervisor drift | Single `nusa` binary owns the edge and PHP supervision |
| RoadRunner as a second product | Octane workers inside the same runtime |
| Security = `disable_functions` hope | Landlock + seccomp before listen |
| Observability bolted on | `/metrics`, structured logs, trace context built in |
| Multi-tenant = ad hoc middleware | Tenant rate limits and circuit breakers in gateway |

You keep **Laravel, Composer, and your application code**.

---

## From PHP-FPM

### Architecture shift

| PHP-FPM | Nusa |
|---------|------|
| nginx terminates TLS, forwards to FPM socket | Nusa gateway terminates HTTP |
| Pool `pm.max_children` in `www.conf` | `max_workers` + backpressure |
| Per-request bootstrap | Normal mode: governed per-request `PhpEngine` |
| `slowlog`, `status` path | `/metrics`, JSON logs, `/health`, `/ready` |

### Steps

1. Start with **`engine = "child"`** and **`octane_workers = 0`** — behavior closest to FPM isolation
2. Map **`code_dir`**, **`vfs_root`**, and **`tmp_dir`** to your Laravel tree
3. Point load balancer health checks to **`GET /health`** and **`GET /ready`**
4. Scrape **`GET /metrics`** into Prometheus
5. Load-test staging; compare p95 latency before cutover

### What feels different

- Upload and cache paths must respect Landlock — plan `tmp_dir` and storage mounts deliberately
- You tune **`max_workers`** and timeouts in one file instead of `pm = dynamic` in FPM pool config
- PHP errors surface as gateway upstream errors with HTTP status mapping

---

## From RoadRunner or FrankenPHP

### Why teams consider Nusa

RoadRunner and FrankenPHP proved that bootstrap-once, serve-many transforms Laravel economics. Nusa embeds that pattern inside a Rust platform that also owns security policy, tenant controls, and cloud-native operability.

| Concept | RoadRunner / FrankenPHP | Nusa |
|---------|-------------------------|------|
| Long-lived workers | Core idea | `octane_workers` + `nusa/octane` |
| HTTP front | Separate service or module | `nusa-gateway` (Axum) |
| Worker reset | Manual / plugin-specific | `StateResetOrchestrator` + memory/request recycle |
| IPC | RR-specific or in-process | Framed nusa-ipc with handshake |

### v0.1.0 caveat

The worker pool, IPC stack, and gateway HTTP dispatch are implemented in v0.1.0. Before production cutover:

- Validate on **Alpine musl** (`just podman-ci`)
- Install **`nusa/octane`** with binary **`nusa-octane-worker`**
- Review [Production status](production-status.md) for benchmark and release KPIs

---

## PHP driver

The bridge from Rust to Laravel is the **`nusa/octane`** Composer package:

1. Add a path or VCS repository to `php-driver/`
2. Require **`nusa/octane`** (`NusaOctaneServiceProvider` auto-discovers)
3. Point Octane at **`nusa-octane-worker`** (`bin/nusa-octane-worker` or `vendor/bin/nusa-octane-worker`)

Setup: [PHP driver package](laravel/php-driver.md).

---

## Security model

Nusa applies **Landlock** and **seccomp** before accepting traffic. That is stronger than most PHP-only hardening but means:

- Paths outside `code_dir` / `tmp_dir` may be denied by design
- Symlink and storage layouts must be reviewed once
- Compliance narratives can point to kernel-enforced boundaries

Schedule a short storage audit during migration to prevent 500s that are actually policy success.

---

## Configuration parity

| Legacy knob | Nusa equivalent |
|-------------|-----------------|
| FPM `pm.max_children` | `max_workers` |
| `PHP_MEMORY_LIMIT` | `octane_max_memory_mb` |
| RR worker count | `octane_workers` |
| Request timeout at proxy | `timeout_ms` at gateway |

---

## Rollback strategy

Keep FPM or RoadRunner warm and automatable until:

- `/ready` reflects your readiness rules
- Load tests meet your SLO on staging
- Operations runbook is exercised

---

## Related documents

- [Production status](production-status.md)
- [Operations runbook](operations/runbook.md)
- [SECURITY.md](../../SECURITY.md)
