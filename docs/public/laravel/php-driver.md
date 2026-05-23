# PHP driver package (`nusa/octane`)

The **`nusa/octane`** Composer package (source in the repo directory **`php-driver/`**) connects Laravel to Nusa’s worker pool. It registers **`Nusa\Octane\NusaOctaneServiceProvider`** and ships the long-lived worker binary **`nusa-octane-worker`** that Nusa spawns in Octane mode.

---

## What it does

| Piece | Role |
|-------|------|
| `NusaOctaneServiceProvider` | Auto-discovered Laravel provider: Octane event listeners, config publish |
| `bin/nusa-octane-worker` | Boots Laravel once, speaks IPC, serves many requests |
| `config/nusa-octane.php` | Optional published config (`php artisan vendor:publish --tag=nusa-config`) |

Rust owns spawn, recycle, and health. PHP owns Laravel inside the sandbox.

---

## Package layout

```text
php-driver/
├── composer.json              # name: nusa/octane
├── bin/
│   └── nusa-octane-worker     # Worker entrypoint (replaces legacy octane-rust-worker)
├── config/
│   └── nusa-octane.php
└── src/
    ├── NusaOctaneServiceProvider.php
    ├── Worker.php
    └── Commands/OctaneStartCommand.php
```

**Naming (v0.1.0):** The worker binary is **`nusa-octane-worker`**. The service provider class is **`NusaOctaneServiceProvider`** in namespace **`Nusa\Octane`**. Older names (`octane-rust-worker`, `OctaneRustServiceProvider`, `NusaRs\OctaneDriver`) are removed.

---

## Install in your Laravel app

### Development (path repository)

If Nusa and your app live on the same machine:

```json
{
  "repositories": [
    {
      "type": "path",
      "url": "../php-driver",
      "options": { "symlink": true }
    }
  ],
  "require": {
    "nusa/octane": "@dev"
  }
}
```

```bash
composer update nusa/octane
```

Laravel auto-discovers **`Nusa\Octane\NusaOctaneServiceProvider`** via `extra.laravel.providers` in the driver’s `composer.json`. You normally do not register it manually in `config/app.php`.

Confirm the binary exists:

```bash
ls -la vendor/nusa/octane/bin/nusa-octane-worker
# or symlink at project root (CI pattern):
ls -la php-driver/bin/nusa-octane-worker
```

Composer also exposes:

```bash
vendor/bin/nusa-octane-worker
```

### Production

At GA, depend on a **semver release** on Packagist aligned with your Nusa version. Until then, vendor the path package or pin a git revision in `composer.json`.

---

## Symlink layout (CI / containers)

Alpine CI uses:

```text
my-laravel-app/
├── php-driver → symlink to repo php-driver/
└── vendor/nusa/octane/...
```

Nusa’s pool spawns the worker relative to `code_dir`, typically:

```text
php-driver/bin/nusa-octane-worker
```

See `crates/nusa-octane-worker/src/pool.rs` and `tests/fixtures/laravel-minimal/`.

---

## Worker health checklist

Before enabling `octane_workers` in production:

- [ ] `composer install --no-dev` in image build  
- [ ] `php artisan config:cache` (optional but recommended)  
- [ ] `code_dir` in `nusa.toml` matches container mount  
- [ ] Manual test: pool `initialize()` succeeds (see fixture tests or staging logs)  
- [ ] `/ready` returns 200 under load  

---

## Laravel Octane package

If you use **`laravel/octane`** for local tooling:

- Set the worker binary to **`nusa-octane-worker`** (not RoadRunner’s `rr`).  
- Disable duplicate HTTP servers—only **one** process should listen (Nusa gateway).  

Nusa already embeds the gateway; you do not run `rr serve` alongside `nusa`.

---

## Version alignment

Keep **`nusa/octane`** on the same release line as the `nusa` binary. IPC handshake assumes compatible message formats.

---

## More detail

Contributor-oriented layout: [PHP ecosystem guidelines](../ecosystem/package-guidelines.md).

---

## Next steps

- [Octane mode](octane-mode.md)  
- [Troubleshooting](troubleshooting.md)  
