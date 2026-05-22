#!/usr/bin/env sh
# P2: Verify Laravel sees `php artisan octane:start --server=nusa` (contract surface).
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PHP="${PHP:-php}"
if command -v php84 >/dev/null 2>&1; then
  PHP=php84
fi

"$PHP" artisan octane:start --help | grep -q -- '--server'
"$PHP" artisan list | grep -q 'octane:start'
echo "artisan octane:start contract OK"
