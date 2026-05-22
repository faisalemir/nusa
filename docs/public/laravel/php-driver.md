# PHP driver package (`nusa/php-driver`)

The **`nusa/php-driver`** Composer package connects Laravel to Nusa’s worker pool. It provides the long-lived worker entrypoint **`octane-rust-worker`** that Nusa spawns in Octane mode.

---

## What it does

| Piece | Role |
|-------|------|
| `bin/octane-rust-worker` | Boots Laravel once, speaks IPC, serves many requests |
| Package classes | Hooks compatible with Octane-style worker lifecycle |

Rust owns spawn, recycle, and health. PHP owns Laravel inside the sandbox.

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
    "nusa/php-driver": "@dev"
  }
}
```

```bash
composer update nusa/php-driver
```

Confirm the binary exists:

```bash
ls -la vendor/nusa/php-driver/bin/octane-rust-worker
# or symlink: php-driver/bin/octane-rust-worker at project root (CI pattern)
```

### Production

At GA, depend on a **semver release** on Packagist aligned with your Nusa version. Until then, vendor the path package or pin a git revision in `composer.json`.

---

## Symlink layout (CI / containers)

Alpine CI uses:

```text
my-laravel-app/
├── php-driver → symlink to repo php-driver/
└── vendor/...
```

Entrypoint invoked as: `php-driver/bin/octane-rust-worker` relative to `code_dir`.

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

- Set the worker binary to Nusa’s `octane-rust-worker` instead of RoadRunner’s `rr`.  
- Disable duplicate HTTP servers—only **one** process should listen (Nusa gateway).  

Nusa already embeds the gateway; you do not run `rr serve` alongside `nusa`.

---

## Version alignment

Keep **`nusa/php-driver`** on the same release line as the `nusa` binary. IPC handshake assumes compatible message formats.

---

## More detail

Contributor-oriented layout: [PHP ecosystem guidelines](../ecosystem/package-guidelines.md).

---

## Next steps

- [Octane mode](octane-mode.md)  
- [Troubleshooting](troubleshooting.md)  
