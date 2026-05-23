#!/usr/bin/env sh
# Prepare laravel-minimal for Nusa E2E tests (Linux/Alpine).
set -eu

ROOT="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$ROOT/../../.." && pwd)"

cd "$ROOT"

ln -sfn "$REPO/php-driver" "$ROOT/php-driver"

if [ ! -f .env ]; then
  cp .env.example .env
fi

mkdir -p storage/framework/cache storage/framework/sessions storage/framework/views storage/logs bootstrap/cache
chmod -R a+rwX storage bootstrap/cache 2>/dev/null || true

if command -v php84 >/dev/null 2>&1; then
  PHP=php84
elif command -v php83 >/dev/null 2>&1; then
  PHP=php83
else
  PHP=php
fi

COMPOSER_ALLOW_SUPERUSER=1 "$PHP" "$(command -v composer)" install --no-interaction --prefer-dist

test -f vendor/autoload.php
test -f php-driver/bin/nusa-octane-worker
echo "Laravel fixture ready at $ROOT"
