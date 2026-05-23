#!/usr/bin/env sh
# Sync php-driver/composer.json "version" from [workspace.package].version in Cargo.toml.
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
cargo_toml="$root/Cargo.toml"
composer_json="$root/php-driver/composer.json"

version="$(awk '
  /^\[workspace\.package\]/ { in_ws=1; next }
  in_ws && /^\[/ { exit }
  in_ws && /^version = "/ {
    gsub(/^version = "|"$/, "", $0)
    print $0
    exit
  }
' "$cargo_toml")"

if [ -z "$version" ]; then
  echo "Could not read [workspace.package].version from $cargo_toml" >&2
  exit 1
fi

if grep -q '"version"' "$composer_json"; then
  sed -i "s/\"version\"[[:space:]]*:[[:space:]]*\"[^\"]*\"/\"version\": \"$version\"/" "$composer_json"
else
  sed -i "s/\"name\"[[:space:]]*:[[:space:]]*\"nusa\\/octane\",/\"name\": \"nusa\/octane\",\n    \"version\": \"$version\",/" "$composer_json"
fi

echo "php-driver/composer.json version -> $version"
