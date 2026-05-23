# Embed runtime image (experimental)

**Status:** Phase 2e — CI uses `nusa-test-runner` + `NUSA_OCTANE_BACKEND=embed`, not a separate production image yet.
**NEB1 frame transport:** default for embed since 2026-05-23; JSON available via `NUSA_EMBED_TRANSPORT=json`.

## Today

| Artifact | Role |
|----------|------|
| `dockerfiles/nusa-test-runner.Dockerfile` | Alpine musl, PHP 8.5 (`php85`), Laravel fixture, Rust tests |
| `just podman-ci-embed` | fmt + lint + workspace tests + IPC E2E + embed pool tests |
| `octane_backend = embed` | Stdio `nusa_embed_daemon.php` (NEB1 frame default; no linked libphp) |

## Target (`nusa-runtime:<semver>`)

Per [RFC](rfc/nusa-native-laravel-runtime.md):

- Alpine musl + **libphp ZTS** linked into `nusa`
- Default env: `NUSA_OCTANE_BACKEND=embed`, `NUSA_OCTANE_WORKERS=4`, `NUSA_CODE_DIR=/app`
- Cargo feature: `embed-php = ["nusa-engine-embed", "nusa-engine-ffi/php_embed_available"]`

Root [`Dockerfile`](../../Dockerfile) sketches a ZTS build (currently PHP 8.3 — update to 8.5 when embed linking lands).

## Build test image (dev)

```bash
just podman-build
just podman-test-laravel-embed
```

## OPcache / JIT (P4-C)

| File | Role |
|------|------|
| [`dockerfiles/php85/99-nusa-opcache.ini`](../dockerfiles/php85/99-nusa-opcache.ini) | CI image: JIT tracing, `validate_timestamps=1` (bind-mount safe) |
| [`dockerfiles/php85/99-nusa-opcache-prod.ini`](../dockerfiles/php85/99-nusa-opcache-prod.ini) | Production: `validate_timestamps=0` |
| [`dockerfiles/warm-php-opcache.sh`](../dockerfiles/warm-php-opcache.sh) | Image build + `podman-laravel-fixture.sh` kernel warm-up |

Packages: `php85-opcache` in `nusa-test-runner.Dockerfile`.

## When libphp embed ships

1. Extend `nusa-engine-ffi` with thread init + `nusa_embed_*` calls.
2. Replace stdio daemon default in production image only (keep IPC for dev/Windows).
3. Seccomp profile audit for embed syscall set.
4. Bake production ini (`99-nusa-opcache-prod.ini`) into `nusa-runtime` tag.
