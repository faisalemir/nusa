# PHP ecosystem and the Nusa driver

Laravel lives in PHP. Nusa lives in Rust. Production excellence requires a **thin, explicit bridge**—not stringly-typed shell scripts—so both sides keep their strengths.

The **`nusa/octane`** Composer package (sources in **`php-driver/`**) is that bridge: Composer-installable, Octane-aware, and designed to speak the **Nusa IPC contract** the Rust worker pool already implements.

---

## Why a first-party PHP driver exists

| Without a driver | With `nusa/octane` |
|------------------|---------------------|
| Ad-hoc `php artisan` wrappers per deploy | One worker entrypoint (`nusa-octane-worker`) the pool spawns predictably |
| Unclear bootstrap boundaries | Laravel `bootstrap/app.php` loaded under Octane semantics |
| Version skew between Rust IPC and PHP | Released together at GA; same handshake version assumptions |
| Security story only on Rust side | PHP obeys the same app root (`code_dir`) Landlock already trusts |

You are not “embedding Rust in PHP.” You are **registering PHP as a governed worker** in a Rust orchestration graph.

---

## Repository layout

```text
php-driver/
├── composer.json              # "name": "nusa/octane"
├── bin/
│   └── nusa-octane-worker     # Long-lived worker: IPC ↔ Laravel bootstrap
├── config/
│   └── nusa-octane.php
└── src/
    ├── NusaOctaneServiceProvider.php
    ├── Worker.php
    └── Commands/OctaneStartCommand.php
```

| Artifact | Purpose |
|----------|---------|
| **`NusaOctaneServiceProvider`** | Registers Octane lifecycle listeners and publishes `nusa-octane` config |
| **`nusa-octane-worker`** | Executable invoked by `WorkerPool` (see `pool.rs`) |

Legacy names (`octane-rust-worker`, `OctaneRustServiceProvider`) are **not** part of the current tree.

The worker is executed with:

- **Application root** = `code_dir` from `nusa.toml`  
- **Transport** = framed messages over `nusa-ipc` (handshake, heartbeat, request/response)  
- **Recycle policy** = `octane_max_memory_mb` and `octane_max_requests` enforced from Rust  

---

## Installing in your Laravel application

During development, use a Composer path repository:

```json
{
  "repositories": [
    {
      "type": "path",
      "url": "../php-driver"
    }
  ],
  "require": {
    "nusa/octane": "@dev"
  }
}
```

At GA, depend on a **semver-tagged Packagist release** aligned with the Rust workspace version.

Run `composer install` in the Laravel tree mounted at `code_dir`.

---

## Worker contract (what “healthy” means)

A worker is production-viable when:

1. **`vendor/autoload.php`** resolves under `code_dir`  
2. **`bootstrap/app.php`** boots an Octane-compatible Laravel application  
3. IPC **Hello / Ack** handshake completes within orchestrator timeouts  
4. Heartbeats continue under load (see `nusa-ipc` tests)  
5. Memory and request counters trigger **recycle** before leak-induced drift  

Rust owns **when** to spawn and kill; PHP owns **how** to serve Laravel requests inside the boundary.

---

## Octane configuration

Point Laravel Octane at the Nusa worker binary instead of RoadRunner’s:

- Binary: `vendor/bin/nusa-octane-worker` or `php-driver/bin/nusa-octane-worker` under `code_dir`  
- Worker count: driven by **`octane_workers`** in `nusa.toml`, not duplicated blindly in `.env`  

HTTP dispatch through the pool is live in v0.1.0 when `pool.is_ready()`. User guide: [Octane mode](../laravel/octane-mode.md).

---

## Third-party Composer packages

Laravel ecosystem packages (Sanctum, Horizon, Nova, etc.) follow normal Composer rules. Nusa does not rewrite Composer—**Rust sandboxing** (Landlock/seccomp) complements PHP-level security; it does not replace dependency review.

Platform teams should still:

- Scan Composer lockfiles in CI  
- Pin extensions in container images  
- Map `storage/` and cache paths into `tmp_dir` policy  

---

## Versioning and API stability

| Stage | Expectation |
|-------|-------------|
| **v0.1.0** | Driver APIs may change; pin path repo commits in pilots |
| **Pre-GA** | Breaking IPC or bootstrap changes announced in CHANGELOG |
| **GA v1.0** | Rust `Cargo.toml` version and Packagist tag move together |

Treat `@dev` constraints as **lab-only**, not fleet-wide.

---

## Verifying integration

Contributors and advanced operators:

```bash
just podman-test-live
```

Requires PHP/Laravel in the test image. Default `just podman-test` may not exercise live PHP—see [contributor testing](../../contributor/testing.md).

---

## Vision: one runtime, two languages, zero ambiguity

The PHP driver is small on purpose. Its job is to make Laravel **feel native** inside Nusa while Rust handles everything that should never have been ini files: **concurrency, kernel policy, metrics, and failure containment**.

With HTTP Octane dispatch wired in v0.1.0, the same package you install today is the **performance story** teams migrated from RoadRunner to capture—without surrendering observability or sandbox discipline.

That is the ecosystem bet: **Laravel’s soul, Rust’s spine.** This package is where they shake hands.
