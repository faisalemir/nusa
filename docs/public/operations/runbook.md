# Operations runbook

This runbook is for **SRE and platform engineers** operating Nusa PHP Runtime in staging and production. Nusa is designed to feel like a **cloud-native Rust service** that happens to execute Laravel—not like a fragile PHP stack duct-taped to nginx.

Adjust commands for your orchestrator (Kubernetes, Nomad, Docker Compose, systemd). Examples assume the **`nusa`** binary and **`nusa.toml`** config.

**Version context:** v0.1.0 pre-GA—see [Production status](../production-status.md) for Octane and readiness caveats.

---

## Service overview

| Attribute | Value |
|-----------|--------|
| Process | `nusa` (`nusa-cli` crate) |
| Config | `nusa.toml` or `--config /path/to/nusa.toml` |
| Production target image | Alpine Linux **musl** (`dockerfiles/nusa-test-runner.Dockerfile`) |
| Primary PHP path (production) | `octane_workers > 0`, `octane_backend = ipc` or `embed` |
| Debug / migration | `octane_workers = 0`, `engine = child` (fork per request) |

One process exposes HTTP, metrics, health, WebSocket/SSE, and task APIs—reducing moving parts during incidents.

---

## Architecture at runtime (operator view)

```
Load balancer
      │  GET /health  (liveness)
      │  GET /ready   (readiness)
      ▼
┌─────────────────────────────────┐
│ nusa (single process)           │
│  • Backpressure / timeouts      │
│  • Global + tenant circuit CB   │
│  • Rate limits per tenant       │
│  • Prometheus /metrics          │
└──────────────┬──────────────────┘
               ▼
        LaravelHttpRuntime (Octane ipc/embed)  OR  PhpEngine (Normal)
```

Readiness reflects **gateway health** and, when `octane_workers > 0`, **worker pool** `is_ready()`.

---

## Health and readiness probes

| Probe | Endpoint | Pass condition | Orchestrator use |
|-------|----------|----------------|------------------|
| **Liveness** | `GET /health` | HTTP 200 | Restart if process wedged |
| **Readiness** | `GET /ready` | HTTP 200 when `HealthState` is ready | Remove from service pool |

### Readiness with Octane (v0.1.0)

When `octane_workers > 0`, **`GET /ready`** returns **503** unless both gateway health is ready **and** `LaravelHttpRuntime::is_ready()` is true (IPC or embed pool). CLI startup **exits** if the pool cannot initialize.

**CI gates:** `just podman-ci` (IPC + workspace) · `just podman-ci-embed` (embed pool + `laravel_embed_pool_ping`).

- Treat pool init errors at startup as **severity 1**  
- Worker binary: **`nusa-octane-worker`** from package **`nusa/octane`** (`php-driver/`)  
- See [PHP driver](../laravel/php-driver.md) and [Octane mode](../laravel/octane-mode.md)

---

## Metrics and alerting

**Endpoint:** `GET /metrics` (Prometheus text exposition)

**Dashboards:** Import JSON from [`docs/monitoring/`](../../monitoring/) into Grafana.

**Suggested alerts (starting set):**

| Signal | Indication |
|--------|------------|
| `requests_failed_total` rate | PHP upstream instability or circuit open |
| Request duration histogram | Latency SLO breach |
| Circuit breaker open (logs + metrics) | Stop cascading overload |
| 429 rate | Tenant abuse or mis-tuned limits |
| Readiness failures | Drain before incident deepens |

Custom metrics are defined in `nusa-telemetry` (`NusaMetrics`). Treat them as first-class SLO inputs, not debugging leftovers.

---

## Logging

- **Production:** structured JSON via `tracing`—ship to your log aggregator; correlate with W3C `traceparent` when present  
- **Development:** `nusa dev --pretty` for human-readable local streams  

During incidents, filter on `engine execution failed` and tenant/circuit messages—the gateway maps domain errors to HTTP statuses explicitly.

---

## Common incidents

### 503 — circuit breaker open

**Symptoms:** `Service Unavailable`, global or **tenant** circuit messages in body/logs.

**Mechanism:** Failures increment breaker state; success path records recovery. Per-tenant breakers isolate noisy neighbors in multi-tenant deployments.

**Response:**

1. Identify PHP fatal loops or timeout storms in logs.  
2. Verify `timeout_ms` is appropriate for route class (API vs report).  
3. Restore upstream (database, cache) before forcing breaker reset via traffic success.  
4. Post-incident: tune breaker thresholds if flapping on transient blips.

---

### 429 — rate limit exceeded

**Symptoms:** `Too Many Requests: Rate Limit Exceeded`

**Mechanism:** `TenantRateLimiter` enforces per-tenant token bucket semantics at the gateway.

**Response:**

1. Extract tenant from `X-Tenant-Id` or host subdomain logic.  
2. Distinguish abuse from legitimate burst (marketing event).  
3. Adjust limits in deployment configuration or upstream WAF—document changes.

---

### Upstream PHP / child engine errors

**Symptoms:** `Upstream Error: …` with 5xx mapping from `EngineError`

**Response:**

1. Confirm PHP binary on `PATH` and `code_dir` contains `vendor/autoload.php`.  
2. Validate file permissions under Landlock (`code_dir`, `tmp_dir`).  
3. Reproduce with single request against `/` and inspect Laravel logs.  
4. Roll back release if regression correlates with deploy time.

---

### Octane pool initialization warning

**Symptoms:** Log line `Failed to initialize Octane worker pool` while process continues.

**Risk:** **High** for any deployment with `octane_workers > 0`—you may be serving without the worker tier you believe exists.

**Response (v0.1.0):**

1. **Stop routing production traffic** until pool healthy or `octane_workers` set to 0.  
2. Fix worker script path, Laravel bootstrap, and IPC socket permissions.  
3. Track P1 release for **fail-closed startup** (process exit non-zero when workers required).

---

### 413 — request too large

**Mechanism:** Gateway middleware rejects bodies over configured limits (default order of tens of MB in tests).

**Response:** Client must chunk uploads or raise limit deliberately in `ResourceGuard` configuration—not by bypassing middleware.

---

## Graceful shutdown

1. Remove instance from load balancer (fail `/ready` or orchestrator drain).  
2. Wait in-flight requests to complete within `timeout_ms`.  
3. Send **SIGTERM** to `nusa`.  
4. Verify process exit and no orphaned PHP children (child engine / future Octane).  

For Kubernetes: use `preStop` sleep aligned with drain interval.

---

## Deployment checklist (staging → production)

- [ ] Release artifact built and tested with **`just podman-ci`** on same musl lineage as prod image  
- [ ] `engine = "child"` unless FFI validated on target  
- [ ] `octane_workers = 0` **or** P1 verified in your environment with pool readiness gating  
- [ ] `code_dir`, `tmp_dir`, volumes aligned with Landlock policy  
- [ ] Prometheus scrape and dashboards imported  
- [ ] Alerts wired for 5xx rate, latency, readiness  
- [ ] Runbook owners named; rollback to FPM/RR documented  
- [ ] Security contact aware ([SECURITY.md](../../../SECURITY.md))

---

## Features not available (do not runbook as live)

| Item | Status |
|------|--------|
| `/admin/recycle-all` HTTP admin | **Not implemented** |
| Production WASM PHP | **Stub in CLI** |
| HTTP admin for manual worker recycle | Use config recycle limits when Octane HTTP path is live |

---

## Escalation

| Type | Contact / doc |
|------|----------------|
| Security vulnerability | [SECURITY.md](../../../SECURITY.md) — do not file public issues |
| Architecture / behavior | [contributor/architecture.md](../../contributor/architecture.md) |
| Maturity questions | [production-status.md](../production-status.md) |

---

## Operator closing note

Nusa rewards teams that operate **one serious binary** instead of five soft configs. The runbook will grow with P1–P4—Octane readiness, Laravel E2E gates, benchmark SLOs—but the discipline here already matches how you run Rust and Go services. That is intentional, and it is why the runtime belongs in production platforms—not only in side projects.
