# PHP 8.5 and Nusa

Nusa CI and the official Alpine test image pin **PHP 8.5.6** (`php85` packages). The `nusa/octane` driver requires **PHP ^8.5**.

## Why 8.5

PHP 8.5 is the runtime Nusa targets for Octane and embed workers:

- **Built-in URI extension** — RFC 3986 parsing for request paths without fragile `parse_url()` edge cases.
- **`array_first()` / `array_last()`** — used when normalizing multi-value headers from IPC.
- **Persistent cURL share handles** — `curl_share_init_persistent()` warms DNS/SSL session state per worker process.
- **`#[\NoDiscard]`** — driver helpers mark return values that must not be ignored.

Future embed builds (`octane_backend = embed` with linked libphp) will use the same PHP minor as the `nusa-runtime` image tag.

## Driver usage

| Component | PHP 8.5 feature |
|-----------|-----------------|
| `Nusa\Octane\Http\RequestPath` | `Uri\Rfc3986\Uri::parse()` when `uri` extension is loaded |
| `Nusa\Octane\Support\IpcBody` | Shared body normalization for IPC + embed |
| `Nusa\Octane\Support\OctaneCurlShare` | Persistent share per `nusa-octane-worker` process |

## Local setup

**Alpine / CI:** `apk add php85 php85-*` (see `dockerfiles/nusa-test-runner.Dockerfile`).

**Linux host:** install PHP 8.5 from your distro or [php.net](https://www.php.net/downloads.php), then:

```bash
cd tests/fixtures/laravel-minimal
./setup-fixture.sh
```

**Windows:** use PHP 8.5 x64 from [windows.php.net](https://windows.php.net/download/); ensure `php` is on `PATH`.

## Verify

```bash
php -v   # PHP 8.5.x
php -m | grep -i uri   # uri (built-in on 8.5)
php -m | grep -i opcache
php -i | grep opcache.jit
```

Alpine CI images install [`dockerfiles/php85/99-nusa-opcache.ini`](../../dockerfiles/php85/99-nusa-opcache.ini) and run [`warm-php-opcache.sh`](../../dockerfiles/warm-php-opcache.sh) at build time.

## See also

- [Compatibility matrix](../compatibility-matrix.md)
- [PHP driver](php-driver.md)
- [Octane mode](octane-mode.md)
