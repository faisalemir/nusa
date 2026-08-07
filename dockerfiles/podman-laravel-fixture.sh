#!/bin/sh
# Prepare Laravel minimal fixture inside the Podman test container.
# Skips `composer install` when vendor/ is already valid (image bake or prior run).
set -eu

# Detect PHP binary (php85, php84, or php)
if [ -x /usr/bin/php85 ]; then
    PHP_BIN=/usr/bin/php85
elif [ -x /usr/bin/php84 ]; then
    PHP_BIN=/usr/bin/php84
else
    PHP_BIN=php
fi
ln -sfn "$PHP_BIN" /usr/bin/php

cd /src/tests/fixtures/laravel-minimal
ln -sfn /src/php-driver ./php-driver
cp -f .env.example .env
mkdir -p storage/framework/cache storage/framework/sessions storage/framework/views \
    storage/logs bootstrap/cache database
touch database/database.sqlite
chmod -R a+rwX storage bootstrap/cache database

need_composer=0
if [ ! -f vendor/autoload.php ]; then
    need_composer=1
elif [ composer.lock -nt vendor/autoload.php ]; then
    need_composer=1
elif [ ! -f vendor/nusa/octane/src/Worker.php ]; then
    need_composer=1
fi

if [ "$need_composer" -eq 1 ]; then
    rm -rf vendor/nusa
    COMPOSER_ALLOW_SUPERUSER=1 /usr/bin/composer install \
        --no-interaction --prefer-dist --no-progress
fi

# Regenerate autoloader for new PSR-4 files (e.g. Embed/FrameCodec)
if [ -f vendor/autoload.php ]; then
    COMPOSER_ALLOW_SUPERUSER=1 /usr/bin/composer dump-autoload \
        --no-interaction --optimize
fi

test -f vendor/autoload.php
test -f vendor/nusa/octane/src/Worker.php
test -f php-driver/bin/nusa-octane-worker

if [ -x /src/dockerfiles/warm-php-opcache.sh ]; then
    # Only warm opcache if PHP version matches fixture requirements
    if php -r 'exit(version_compare(PHP_VERSION, "8.5.0", ">=") ? 0 : 1);' 2>/dev/null; then
        NUSA_LARAVEL_FIXTURE=/src/tests/fixtures/laravel-minimal /src/dockerfiles/warm-php-opcache.sh
    else
        echo "warm-php-opcache: skip (PHP $(php -r 'echo PHP_VERSION;') < 8.5)"
    fi
fi
