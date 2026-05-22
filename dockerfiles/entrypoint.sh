#!/bin/sh
set -e

# If source was mounted, copy to native filesystem for speed
if [ -d "/src/crates" ]; then
    echo "Copying source from mount to native /app..."
    cp -r /src/. /app/
fi

cd /app
rm -f .cargo/config.toml
echo "Running tests on native filesystem..."
exec cargo nextest run --workspace --test-threads 4
