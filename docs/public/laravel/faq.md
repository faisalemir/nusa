# FAQ (Laravel on Nusa)

---

## Do I need to learn Rust?

No. You configure **`nusa.toml`**, install **`nusa/octane`** for Octane mode, and run Laravel as usual.

---

## Is this “Laravel Octane”?

Nusa provides **Octane-style long-lived workers** with its own gateway and sandbox. You may use the `laravel/octane` package for some tooling, but the HTTP server is **`nusa`**, not `rr` or FrankenPHP listening separately.

---

## Normal mode vs Octane mode?

| | Normal | Octane |
|---|--------|--------|
| Config | `octane_workers = 0` | `octane_workers > 0` |
| Feels like | php-fpm | Laravel Octane / RoadRunner |
| Isolation | Per request | Per worker (watch globals) |

→ [Normal mode](normal-mode.md) · [Octane mode](octane-mode.md)

---

## Can I use nginx in front?

Yes. Terminate TLS at nginx and reverse-proxy to `nusa:8080`. Use `/health` and `/ready` for upstream health checks.

---

## Where do `.env` variables go?

**Laravel** settings stay in `.env`. **Runtime** settings use `nusa.toml` or `NUSA_*` env vars.

---

## Does Valet/Herd/XAMPP conflict?

Run **either** those stacks **or** Nusa on the same port—not both. Typical dev: Nusa on 8080, Valet on another project.

---

## Is v0.1.0 safe for production?

**Pilot/staging: yes** with eyes open. **Full GA:** after your Alpine staging sign-off, published benchmarks, and [Production status](../production-status.md) criteria.

---

## Why does startup fail when I set `octane_workers`?

**Fail-closed by design.** Nusa will not listen with a broken worker pool. Fix PHP, Composer, and the driver—see [Troubleshooting](troubleshooting.md).

---

## How do I run tests?

**Laravel:** `php artisan test` as usual.

**Nusa runtime:** contributors use `just podman-ci`; operators validate staging on Alpine.

---

## More help

- [Laravel hub](README.md)  
- [Troubleshooting](troubleshooting.md)  
- [Migration](../migration.md)  
