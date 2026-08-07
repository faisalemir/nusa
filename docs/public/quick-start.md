# Quick start (Laravel + Nusa)

Get a Laravel app answering HTTP through Nusa in about **10 minutes** on Linux or WSL. macOS/Windows builds work for local dev; **production validation** should use Alpine (see [Production status](production-status.md)).

---

## What you need

| Requirement | Version / notes |
|-------------|-----------------|
| **PHP** | 8.5+ (8.5.6 in CI; matches Alpine `php85` image) |
| **Composer** | 2.x |
| **Rust** | 1.95+ ([`rust-toolchain.toml`](../../rust-toolchain.toml)) |
| **Laravel app** | Existing project or new app |

---

## Step 1 — Build `nusa`

From the Nusa repository root:

```bash
cargo build -p nusa-cli --release
```

Binary path: `target/release/nusa` (or `cargo run -p nusa-cli --` for dev).

---

## Step 2 — Point config at your Laravel root

```bash
cp config.toml.example nusa.toml
```

Edit **`code_dir`** to your Laravel project root (where `artisan` lives):

```toml
code_dir = "/home/you/my-laravel-app"
vfs_root = "/home/you/my-laravel-app/public"
tmp_dir  = "/tmp/nusa-myapp"
```

Create the temp directory:

```bash
mkdir -p /tmp/nusa-myapp
```

See [Configuration for Laravel](laravel/configuration-for-laravel.md) for `storage/` and Landlock paths.

---

## Step 3 — Normal mode first (simplest)

Leave Octane off while you learn the runtime:

```toml
engine = "child"
octane_workers = 0
max_workers = 4
timeout_ms = 30000
```

---

## Step 4 — Start the server

```bash
./target/release/nusa --config nusa.toml
```

Or with hot reload while developing:

```bash
cargo run -p nusa-cli -- dev --pretty
```

---

## Step 5 — Verify probes and a page

```bash
curl -s http://127.0.0.1:8080/health
curl -s http://127.0.0.1:8080/ready
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:8080/
```

- **`/health`** — process is up  
- **`/ready`** — ready for traffic (with Octane, requires a healthy worker pool)  
- **`/`** — hits your Laravel front controller path via the PHP engine  

---

## Step 6 (optional) — Enable Octane workers

When Normal mode works:

1. Install the PHP driver: [PHP driver package](laravel/php-driver.md)  
2. Set `octane_workers = 2` (or more) in `nusa.toml`  
3. Restart `nusa` — startup **fails closed** if PHP worker cannot start  
4. Confirm `/ready` returns 200  

Full walkthrough: [Octane mode](laravel/octane-mode.md).

---

## Common first mistakes

| Symptom | Fix |
|---------|-----|
| Permission denied on `storage/` | Align `tmp_dir` and volumes — [Troubleshooting](laravel/troubleshooting.md) |
| `/ready` 503 with Octane enabled | Pool not ready — check PHP path and driver install |
| 404 on all routes | Check `vfs_root` points at `public/` |
| Works on Mac, fails in prod | Run staging on Alpine; use `just podman-ci` parity |

---

## Where to go next

| Goal | Document |
|------|----------|
| Full Laravel guide | [laravel/README.md](laravel/README.md) |
| Every config key | [configuration.md](configuration.md) |
| Migrate from FPM / RoadRunner | [migration.md](migration.md) |
| Production readiness | [production-status.md](production-status.md) |
