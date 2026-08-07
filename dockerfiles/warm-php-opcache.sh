#!/bin/sh
# Warm OPcache/JIT for the Laravel minimal fixture during image build (P4-C).
set -eu

ROOT="${NUSA_LARAVEL_FIXTURE:-/src/tests/fixtures/laravel-minimal}"
cd "$ROOT"

if [ ! -f vendor/autoload.php ]; then
    echo "warm-php-opcache: skip (vendor missing)"
    exit 0
fi

echo "warm-php-opcache: bootstrapping Laravel at $ROOT"
php -r '
$root = getenv("NUSA_LARAVEL_FIXTURE") ?: "/src/tests/fixtures/laravel-minimal";
require $root . "/vendor/autoload.php";
$app = require $root . "/bootstrap/app.php";
$kernel = $app->make(Illuminate\Contracts\Http\Kernel::class);
$kernel->bootstrap();
echo "opcache warm: kernel booted\n";
if (function_exists("opcache_get_status")) {
    $st = opcache_get_status(false);
    if (is_array($st) && isset($st["opcache_enabled"])) {
        echo "opcache_enabled=" . ($st["opcache_enabled"] ? "1" : "0") . "\n";
    }
}
'

echo "warm-php-opcache: done"
