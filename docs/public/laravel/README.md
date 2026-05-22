# Laravel on Nusa

This section is the **primary documentation** for teams building and shipping Laravel on Nusa. It is organized by task—not by Rust crate names.

---

## Learning paths

### Path A — New to Nusa (recommended)

1. [Quick start](../quick-start.md)  
2. [Installation](installation.md)  
3. [Run your existing app](first-app.md)  
4. [Configuration for Laravel](configuration-for-laravel.md)  
5. [Local development](local-development.md)  

### Path B — Coming from Laravel Octane / RoadRunner

1. [Octane mode](octane-mode.md)  
2. [PHP driver package](php-driver.md)  
3. [HTTP, routes, headers](routes-http-and-headers.md)  
4. [Migration from RoadRunner](../migration.md#from-roadrunner-or-frankenphp)  

### Path C — Coming from php-fpm + nginx

1. [Normal mode](normal-mode.md)  
2. [Configuration for Laravel](configuration-for-laravel.md)  
3. [Migration from PHP-FPM](../migration.md#from-php-fpm)  
4. [Deploy checklist](deployment-checklist.md)  

### Path D — Production launch

1. [Production status](../production-status.md)  
2. [Compatibility matrix](../compatibility-matrix.md)  
3. [Deploy checklist](deployment-checklist.md)  
4. [Operations runbook](../operations/runbook.md)  
5. [Troubleshooting](troubleshooting.md)  

---

## Guide index

| Guide | You will learn |
|-------|----------------|
| [Installation](installation.md) | Requirements, build, directory layout |
| [Run your existing app](first-app.md) | Wire `code_dir`, first successful request |
| [Configuration for Laravel](configuration-for-laravel.md) | Paths, env, Octane settings for real apps |
| [Normal mode](normal-mode.md) | FPM-like request lifecycle |
| [Octane mode](octane-mode.md) | Worker pool, readiness, performance |
| [PHP driver package](php-driver.md) | Composer install and worker binary |
| [HTTP, routes, headers](routes-http-and-headers.md) | How requests reach Laravel |
| [Local development](local-development.md) | `nusa dev`, watching files |
| [Deploy checklist](deployment-checklist.md) | Staging and production steps |
| [Troubleshooting](troubleshooting.md) | Errors, 503, permissions, IPC |
| [FAQ](faq.md) | Quick answers |

---

## Concepts Laravel developers should know

| Nusa term | Laravel analogy |
|-----------|-----------------|
| `code_dir` | Project root (`artisan`, `app/`, `config/`) |
| `vfs_root` | Usually `public/` (front controller) |
| `tmp_dir` | Writable area for cache, sessions, uploads (policy-enforced) |
| Normal mode | Like php-fpm: new request context each time |
| Octane mode | Like `php artisan octane:start` with Nusa-managed workers |
| `/ready` | Load balancer “can this instance take traffic?” |
| `nusa/php-driver` | Worker entrypoint instead of `rr` or `frankenphp` binary |

You do **not** need to read Rust source to run Laravel on Nusa.

---

## Reference elsewhere

- Full key list: [Configuration reference](../configuration.md)  
- Platform maturity: [Production status](../production-status.md)  
- PHP/Laravel/OS versions: [Compatibility matrix](../compatibility-matrix.md)  
