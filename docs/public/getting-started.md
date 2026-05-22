# Getting started

> **This page moved.** The public documentation is now organized for **Laravel developers**.

| Start here | Link |
|------------|------|
| **10-minute setup** | [Quick start](quick-start.md) |
| **Full Laravel guide** | [Laravel documentation hub](laravel/README.md) |
| **Public docs home** | [README](README.md) |

---

## Short version

1. Build: `cargo build -p nusa-cli --release`  
2. Config: `cp config.toml.example nusa.toml` — set `code_dir`, `vfs_root`, `tmp_dir`  
3. Run: `nusa --config nusa.toml` or `nusa dev --pretty`  
4. Probe: `curl http://127.0.0.1:8080/health` and `/ready`  

Octane mode: install [PHP driver](laravel/php-driver.md), set `octane_workers > 0`, read [Octane mode](laravel/octane-mode.md).

Production maturity: [Production status](production-status.md).

Contributors: [contributor docs](../contributor/README.md).
