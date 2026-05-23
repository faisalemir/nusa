# Octane mode

**Octane mode** runs Laravel with **long-lived PHP workers** managed by Nusa—similar to **Laravel Octane**, **RoadRunner**, or **FrankenPHP**, but orchestrated by the same `nusa` binary that serves HTTP.

Enable with:

```toml
octane_workers = 4   # example: adjust to your CPU and memory
octane_max_memory_mb = 512
octane_max_requests = 1000
```

---

## Prerequisites

| Requirement | Why |
|-------------|-----|
| [PHP driver installed](php-driver.md) | Package `nusa/octane`, binary `nusa-octane-worker` |
| `composer install` under `code_dir` | Laravel bootstrap |
| Linux staging (recommended) | Matches production sandbox + CI |
| Enough RAM | ~512 MB × workers (rule of thumb—measure your app) |

---

## How it works

```text
HTTP request
    → nusa gateway
    → WorkerPool (when pool.is_ready())
    → IPC (method, URI, body, headers, trace context)
    → PHP worker (Laravel already bootstrapped)
    → IPC response
    → HTTP response
```

When the pool is **not** ready, the gateway returns **503** for application routes (fail-closed).

---

## Startup behavior (fail-closed)

With `octane_workers > 0`:

1. Nusa spawns workers using `php-driver/bin/nusa-octane-worker` (or `vendor/nusa/octane/bin/nusa-octane-worker`).  
2. Each worker completes an IPC **handshake**.  
3. If initialization fails or no worker has transport, **`nusa` exits** with an error.  
4. **`/ready`** stays **503** until `pool.is_ready()` is true.  

You will not get a running server that silently falls back to broken stubs.

---

## Readiness and load balancers

```bash
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:8080/ready
```

| Code | Meaning |
|------|---------|
| **200** | Gateway healthy **and** Octane pool ready |
| **503** | Do not send traffic—workers missing or unhealthy |

Configure Kubernetes `readinessProbe` on `/ready`, not only `/health`.

---

## Worker recycling

Workers restart automatically when:

- Handled requests ≥ `octane_max_requests`, or  
- Resident memory ≥ `octane_max_memory_mb`  

This limits slow leaks in long-lived Laravel processes (static caches, accidental globals).

---

## Laravel Octane config

Point Octane at the Nusa worker binary (when using Laravel’s Octane package for tooling):

- Worker command: `php vendor/bin/nusa-octane-worker` (path depends on Composer layout)  

Your `config/octane.php` may still list RoadRunner options—replace worker command with Nusa’s driver per [PHP driver package](php-driver.md).

---

## Validated behaviors (CI fixture)

The minimal fixture exercises:

| Scenario | Route / check |
|----------|----------------|
| Bootstrap + IPC | Pool `initialize()` + `is_ready()` |
| HTTP body | `GET /` → `nusa-fixture-ok` |
| Multi-worker | `GET /nusa-ping` on 2 workers |
| POST body | `POST /nusa-echo` |
| Query string | `GET /nusa-query?q=foo` |
| Worker-local state | `GET /nusa-counter` increments per worker |
| Gateway integration | HTTP via Axum without calling mock engine |
| Leak suite | 10k sequential requests (pre-GA gate) |

Run full E2E on Alpine: `just podman-ci` / `just podman-ci-e2e`.

---

## When Octane mode is not enough alone

- **Sessions** — use `redis` or database sessions for multi-instance deployments.  
- **Uploaded files** — use object storage or shared volumes aligned with `tmp_dir`.  
- **Queues / Horizon** — run as separate processes (same as other Octane deployments).  
- **Scheduled tasks** — `php artisan schedule:run` via cron/K8s CronJob, not inside workers.  

---

## Troubleshooting

→ [Troubleshooting](troubleshooting.md#octane-mode)

---

## Next steps

- [PHP driver package](php-driver.md)  
- [HTTP, routes, headers](routes-http-and-headers.md)  
- [Deploy checklist](deployment-checklist.md)  
