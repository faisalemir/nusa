# Migration guide

Moving Laravel to Nusa is a **platform upgrade**, not a find-and-replace of `public/index.php`. You gain a unified edge (HTTP, metrics, tenancy, circuit breaking, kernel sandbox) and you retire a patchwork of web server, FPM pool, and process manager configs.

This guide walks **from**, **to**, and **what changes in your mental model**. Octane HTTP dispatch is implemented in v0.1.0—validate on Alpine staging before production cutover.

**Laravel-focused guides:** [laravel/README.md](laravel/README.md).

---

## What you are migrating toward

| Legacy pain | Nusa answer |
|-------------|-------------|
| nginx + php-fpm + Supervisor drift | Single `nusa` binary owns the edge and PHP supervision |
| RoadRunner as a second product to operate | Octane workers over **one IPC contract** inside the same runtime |
| Security = `disable_functions` hope | **Landlock + seccomp** before listen |
| Observability bolted on | `/metrics`, structured logs, trace context built in |
| Multi-tenant = ad hoc middleware | Tenant rate limits and circuit breakers in gateway state |

You keep **Laravel, Composer, and your application code**. You change **who protects the boundary**.

---

## From PHP-FPM

### Architecture shift

| PHP-FPM world | Nusa world |
|---------------|------------|
| nginx terminates TLS, forwards to FPM socket | Nusa gateway terminates HTTP (TLS modules available in crate) |
| Pool `pm.max_children` in `www.conf` | `max_workers` + backpressure in Rust |
| Per-request bootstrap (typical) | Normal mode: governed per-request `PhpEngine` path |
| `slowlog`, `status` path | `/metrics`, JSON logs, `/health`, `/ready` |

### Recommended migration path

1. **Containerize on Alpine musl** if production matches CI (`just podman-ci` parity).  
2. Start with **`engine = "child"`** and **`octane_workers = 0`**—behavior closest to FPM isolation.  
3. Map **`code_dir`**, **`vfs_root`**, and **`tmp_dir`** to your real Laravel tree (see [Configuration](configuration.md)).  
4. Point load balancer health checks to **`GET /health`** (liveness) and **`GET /ready`** (readiness).  
5. Scrape **`GET /metrics`** into Prometheus; retire FPM `status` scraping where redundant.  
6. Load-test staging; compare p95 latency and error rates before cutover.  

### What feels different on day one

- Upload and cache paths must respect **Landlock**—plan `tmp_dir` and storage mounts deliberately.  
- PHP errors surface as gateway **upstream errors** with HTTP status mapping—tune alerts accordingly.  
- You will not configure `pm = dynamic` in FPM; you tune **`max_workers`** and timeouts in one file.

---

## From RoadRunner or FrankenPHP

### Why teams consider Nusa after long-lived workers

RoadRunner and FrankenPHP proved that **bootstrap once, serve many** transforms Laravel economics. Nusa embraces that lesson and **embeds it inside a Rust platform** that also owns security policy, tenant controls, and cloud-native operability—without shipping a separate Go or C server you operate independently.

| Concept | RoadRunner / FrankenPHP | Nusa |
|---------|-------------------------|------|
| Long-lived workers | Core idea | `octane_workers` + `php-driver` worker |
| HTTP front | Separate service or module | `nusa-gateway` (Axum) |
| Worker reset | Manual / plugin-specific | `StateResetOrchestrator` + memory/request recycle config |
| IPC | RR-specific or in-process | Framed **nusa-ipc** with handshake and heartbeat |

### v0.1.0 caveat (read before cutover)

The **worker pool and IPC stack exist** and are tested. The **gateway HTTP handler** still routes through `engine.execute` until **P1** connects pool readiness to request dispatch.

Until your environment validates P1:

- Treat Nusa as **Normal mode + worker bootstrap rehearsal** for Octane  
- Do not expect identical RR throughput numbers on HTTP alone  
- Use [Production status](production-status.md) as the gate for “Octane production ready”

After P1, the same `nusa.toml` you staged should unlock the throughput story without swapping binaries.

---

## Composer and the PHP driver

The bridge from Rust orchestration to Laravel bootstrap is **`nusa/php-driver`**:

1. Add a path or VCS repository to `php-driver/` (see [ecosystem/package-guidelines.md](ecosystem/package-guidelines.md)).  
2. Require the package in your Laravel app.  
3. Configure Octane (when HTTP dispatch is live) to use **`octane-rust-worker`**.  

The worker expects a real Laravel tree: `vendor/`, `bootstrap/app.php`, Octane-compatible boot.

---

## Security model changes (benefits and work)

Nusa applies **Landlock** and **seccomp** from Rust **before** accepting traffic. That is stronger than most PHP-only hardening—but it means:

- Paths outside `code_dir` / `tmp_dir` may be **denied by design**  
- Symlink and storage layouts must be reviewed once, not per deploy argument  
- Compliance narratives can point to **kernel-enforced** boundaries (see [threat model](../security/threat-model.md))

Schedule a short storage audit during migration; it prevents mysterious 500s that are actually policy success.

---

## Configuration parity cheat sheet

| Legacy knob | Nusa equivalent |
|-------------|-----------------|
| `PHP_MEMORY_LIMIT` | `octane_max_memory_mb` (workers); engine-specific for child |
| FPM `pm.max_children` | `max_workers` |
| RR worker count | `octane_workers` |
| Request timeout at proxy | `timeout_ms` at gateway |

---

## Rollback strategy

Keep FPM or RoadRunner **warm and automatable** until:

- `/ready` reflects your readiness rules (including future Octane pool checks)  
- Load tests meet your SLO on staging  
- Runbooks in [operations/runbook.md](operations/runbook.md) are exercised once  

Blue-green flows via `nusa deploy` / `rollback` subcommands align with this strategy as they mature—treat them as companions to orchestrator-native rolls.

---

## Support and honesty

- [Production status](production-status.md) — maturity truth  
- [Operations runbook](operations/runbook.md) — incidents and probes  
- [SECURITY.md](../../SECURITY.md) — vulnerabilities  

We would rather slow your migration by a sprint than mislabel readiness. The architecture you are moving toward is **worth that patience**—and this guide is written so you see both the destination and the last miles of road.
