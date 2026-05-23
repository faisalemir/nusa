# Test fixtures

## Laravel minimal (`laravel-minimal/`)

End-to-end fixture for **P2**: real `vendor/`, `bootstrap/app.php`, and `php-driver/bin/nusa-octane-worker`.

### Layout (after setup)

```
laravel-minimal/
├── php-driver/          → symlink to ../../../php-driver
├── vendor/              → composer install
├── bootstrap/app.php
├── public/
└── routes/web.php       → GET / returns "nusa-fixture-ok"
```

### Local setup (Linux / WSL)

```bash
cd tests/fixtures/laravel-minimal
ln -sfn "$(pwd)/../../../php-driver" php-driver
cp -n .env.example .env
composer install --no-interaction
php artisan key:generate --force  # if APP_KEY invalid
```

### Alpine CI

`dockerfiles/nusa-test-runner.Dockerfile` runs the same steps during image build.  
Run live tests:

```bash
just podman-test-laravel
# or full gate:
just podman-ci
```

### Environment

| Variable | Purpose |
|----------|---------|
| `NUSA_LARAVEL_FIXTURE` | Override fixture root (set in test image) |
