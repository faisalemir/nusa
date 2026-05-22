# Normal mode (FPM-like)

**Normal mode** is the simplest way to run Laravel on Nusa: **`octane_workers = 0`**.

Each HTTP request is handled through the PHP **child engine**—similar in isolation to **php-fpm**, but supervised by Nusa’s gateway (timeouts, metrics, sandbox, rate limits).

---

## When to use Normal mode

| Choose Normal mode when… |
|-------------------------|
| Migrating from **nginx + php-fpm** |
| You want **maximum isolation** per request |
| Octane/RoadRunner is not required yet |
| Debugging Laravel bootstrap issues (cleaner stack traces per request) |
| Running apps with heavy per-request memory spikes |

| Consider Octane mode when… |
|----------------------------|
| You need **warm framework bootstrap** and high RPS |
| You already run **Laravel Octane** or RoadRunner |
| Staging proves worker pool stability under load |

---

## Configuration

```toml
engine = "child"
octane_workers = 0
```

Other keys still apply: `max_workers`, `timeout_ms`, `code_dir`, `vfs_root`, `tmp_dir`.

---

## Request flow

```text
HTTP request
    → nusa gateway (middleware, limits, tracing)
    → PhpEngine::execute (child PHP process path)
    → Laravel (bootstrap + route)
    → HTTP response
```

You do not manage FPM pool files. Tune **`max_workers`** instead of `pm.max_children`.

---

## Health checks

| Endpoint | Normal mode behavior |
|----------|----------------------|
| `/health` | Process alive |
| `/ready` | Ready when gateway health state is OK (no worker pool required) |

Load balancers can use `/ready` once Laravel boots reliably.

---

## Laravel implications

- **Service providers** run per request (unless you cache config/routes in production as usual).  
- **Singleton state** does not leak between requests (unlike Octane).  
- **`.env`** changes may require restart depending on how PHP opcache caches—same as FPM.  

Use standard Laravel production optimizations:

```bash
php artisan config:cache
php artisan route:cache
php artisan view:cache
```

Run these inside your deploy image or init container—not on every request.

---

## Performance expectations

Normal mode trades throughput for simplicity. Benchmark your routes in staging; compare to Octane mode on the same hardware before choosing.

Templates: [Normal mode benchmark report](../benchmarks/normal-mode-report.md) (fill before GA).

---

## Upgrade path to Octane

1. Prove Normal mode stable in staging.  
2. [Install PHP driver](php-driver.md).  
3. Enable `octane_workers` incrementally.  
4. Watch `/ready` and recycle metrics.  

→ [Octane mode](octane-mode.md)
