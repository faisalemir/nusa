# Installation

Install the Nusa binary and prepare your Laravel project paths before the first `nusa` start.

---

## Requirements

| Component | Laravel on Nusa |
|-----------|-----------------|
| **PHP** | 8.2+ with extensions your app needs (`mbstring`, `openssl`, `pdo`, etc.) |
| **Composer** | 2.x — `vendor/` must exist under `code_dir` |
| **OS (production)** | **Alpine Linux musl** is the reference platform |
| **OS (local dev)** | Linux, WSL, macOS, Windows — build `nusa`; sandbox proofs are strongest on Linux |
| **Rust** | Only to **build** `nusa` from source (or use a prebuilt release when published) |

Check [Compatibility matrix](../compatibility-matrix.md) for Laravel 10/11 and PHP 8.4 CI coverage.

---

## Build the `nusa` binary

Clone the repository and build the CLI crate:

```bash
cargo build -p nusa-cli --release
```

Verify:

```bash
./target/release/nusa --help
```

Subcommands include **`dev`** (watch + serve) and default **serve** with `--config`.

---

## Laravel project layout

Nusa expects a standard Laravel tree:

```text
my-laravel-app/          ← code_dir (application root)
├── app/
├── bootstrap/
├── config/
├── public/              ← usually vfs_root
│   └── index.php
├── routes/
├── storage/             ← must be writable (see tmp_dir / volumes)
├── vendor/              ← composer install required
├── artisan
└── composer.json
```

| Config key | Typical value |
|------------|---------------|
| `code_dir` | `/path/to/my-laravel-app` |
| `vfs_root` | `/path/to/my-laravel-app/public` |
| `tmp_dir` | Dedicated writable path (e.g. `/tmp/nusa-myapp` or a mounted volume) |

**Rule:** `code_dir` is where `artisan` lives—not the `public/` folder alone.

---

## Create `nusa.toml`

```bash
cp config.toml.example nusa.toml
```

Minimum for Normal mode:

```toml
engine = "child"
octane_workers = 0
code_dir = "/path/to/my-laravel-app"
vfs_root = "/path/to/my-laravel-app/public"
tmp_dir  = "/tmp/nusa-myapp"
max_workers = 8
timeout_ms = 30000
hot_reload = true
```

Environment overrides use prefix **`NUSA_`** (e.g. `NUSA_CODE_DIR` in Kubernetes).

---

## PHP driver (Octane mode only)

If you plan **`octane_workers > 0`**, install **`nusa/octane`** (from `php-driver/`) before enabling workers:

→ [PHP driver package](php-driver.md)

Without the driver, Nusa **refuses to start** with workers configured (fail-closed).

---

## Verify installation

```bash
mkdir -p /tmp/nusa-myapp
./target/release/nusa --config nusa.toml
```

In another terminal:

```bash
curl -s http://127.0.0.1:8080/health
curl -s http://127.0.0.1:8080/ready
```

---

## Container / Alpine (production-shaped)

For the same environment CI uses:

```bash
just podman-build    # once
just podman-ci       # validates runtime on Alpine musl
```

Operators mirror this image in production. Laravel developers should ensure **`composer install`** runs in the image build or entrypoint for `code_dir`.

---

## Next steps

- [Run your existing app](first-app.md)  
- [Configuration for Laravel](configuration-for-laravel.md)  
- [Quick start](../quick-start.md)  
