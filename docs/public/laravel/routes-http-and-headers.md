# HTTP, routes, and headers

How HTTP requests from clients reach your Laravel routes when Nusa is in front.

---

## URL mapping

Nusa’s gateway listens on **`0.0.0.0:8080`** by default (configurable in future releases via env).

| Client request | Typical handling |
|----------------|------------------|
| `GET /` | `public/index.php` → Laravel front controller |
| `GET /api/users` | Same — Laravel routing as usual |
| `POST /webhook` | Body forwarded to PHP (Octane: full body over IPC) |

**Configure `vfs_root`** to your `public/` directory so script resolution matches nginx `root` behavior.

---

## Methods and bodies

Supported methods follow what Laravel and the IPC layer accept: **GET, POST, PUT, PATCH, DELETE, OPTIONS, HEAD**, etc.

| Mode | Body handling |
|------|----------------|
| Normal | Standard PHP SAPI input |
| Octane | Request body serialized over IPC to the worker |

Fixture test `POST /nusa-echo` verifies body round-trip in CI.

---

## Query strings

Query parameters on the URL are passed through to Laravel:

```http
GET /search?q=laravel&page=2
```

Fixture: `GET /nusa-query?q=foo` → body `q=foo`.

---

## Headers

### Incoming

Custom headers (e.g. `Authorization`, `X-Request-Id`) are forwarded to the worker path where the stack supports them.

**W3C Trace Context:** `traceparent` / `tracestate` are extracted at the gateway and propagated on IPC requests for distributed tracing.

### Outgoing

Laravel response headers from PHP are mapped back to the HTTP response (status, `Content-Type`, cookies set by Laravel, etc.).

---

## Platform routes (not Laravel)

These are served by Nusa directly—do not define them in `routes/web.php`:

| Path | Purpose |
|------|---------|
| `/health` | Liveness |
| `/ready` | Readiness (includes Octane pool when enabled) |
| `/metrics` | Prometheus metrics |
| `/ws` | WebSocket upgrade |
| `/sse` | Server-Sent Events |
| `/api/tasks` | Async task offload API |

---

## Multi-tenant header

Gateway supports tenant routing via **`X-Tenant-Id`** (and host-based routing in multi-tenant setups). Apply Laravel tenancy packages **after** you confirm base routing works in Normal mode.

---

## Static files

Static assets under `public/` may be served by Nusa’s static file handler when configured. Prefer CDN/nginx for cache-heavy assets in large deployments; use Nusa static serving for simpler topologies.

---

## Timeouts

`timeout_ms` in `nusa.toml` caps how long the gateway waits for PHP/Octane. Long-running Laravel requests (reports, exports) need either:

- Higher `timeout_ms`, or  
- Async pattern (`/api/tasks` or Laravel queues)  

---

## Next steps

- [Octane mode](octane-mode.md)  
- [Configuration for Laravel](configuration-for-laravel.md)  
- [Troubleshooting](troubleshooting.md)  
