# Migration Guide: FPM/RoadRunner → Nusa PHP Runtime

## Overview

This guide covers migrating from PHP-FPM or RoadRunner (Swoole) to the Nusa PHP Runtime. Nusa is a Rust-orchestrated runtime that serves Laravel applications with better performance, memory safety, and cloud-native observability.

---

## Part 1: Migrating from PHP-FPM

### Step 1: Prerequisites

```bash
# Install Rust 1.95+
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Ensure PHP 8.2+ is installed (for child process engine)
php -v

# Optional: PHP ZTS with --enable-embed (for FFI engine)
php-config --configure-options | grep zts
```

### Step 2: Install Nusa Octane Driver

```bash
composer require nusa/octane
php artisan vendor:publish --provider="Nusa\Octane\NusaOctaneServiceProvider" --tag=nusa-config
```

### Step 3: Replace nginx/php-fpm config

Remove your nginx/php-fpm configuration and replace with Nusa:

```bash
# Copy example config
cp config.toml.example config.toml

# Start Nusa
cargo build --release
./target/release/nusa --config config.toml
```

### Step 4: Driver Alignment

Nusa requires persistent drivers (no file-based sessions/cache in Octane mode):

```php
// .env — change from file drivers
SESSION_DRIVER=redis
CACHE_DRIVER=redis
QUEUE_CONNECTION=redis

// If using file-based sessions, migrate data
php artisan session:table
php artisan migrate
```

### Step 5: Audit Stateful Packages

Packages that use `register_shutdown_function()` or global statics need adapters:

```php
// BEFORE: package using register_shutdown_function
register_shutdown_function(function () { /* cleanup */ });

// AFTER: use Octane events
use Laravel\Octane\Events\RequestTerminated;
Event::listen(RequestTerminated::class, function ($event) {
    /* cleanup */
});
```

### Step 6: Test

```bash
# Run Nusa test suite
./target/release/nusa test --path tests

# Load test
wrk -t4 -c100 -d30s http://localhost:8080/
```

---

## Part 2: Migrating from RoadRunner

### Step 1: Remove RoadRunner

```bash
# Remove spiral/roadrunner and related packages
composer remove spiral/roadrunner spiral/roadrunner-http spiral/roadrunner-cli

# Remove .rr.yaml
rm .rr.yaml
```

### Step 2: Install Nusa

```bash
composer require nusa/octane
```

### Step 3: Update Octane Config

```php
// config/octane.php
'server' => 'nusa', // was 'roadrunner'
```

### Step 4: Update CI/CD

```yaml
# Before (RoadRunner)
- name: Download RoadRunner
  run: ./vendor/bin/rr get-binary

# After (Nusa)
- name: Build Nusa Runtime
  run: cargo build --release --bin nusa
```

---

## Part 3: Compatibility Matrix

| Laravel Component | PHP-FPM | RoadRunner | Nusa | Notes |
|---|---|---|---|---|
| Sessions | file/redis/cookie | redis/memcached | redis/memcached | File drivers not safe |
| Cache | any | any | redis/memcached | File drivers not safe |
| Queues | any | any | any | Fully compatible |
| Middleware | all | all | all | Fully compatible |
| Artisan | full | full | full | `nusa` CLI replaces `rr` |
| Broadcasters | pusher/redis | pusher/redis | pusher/redis | + native WebSocket |
| Packages | ~100% | ~90% | ~95%+ | Static-cache packages need adapters |

### Known Incompatible Packages

| Package | Issue | Fix |
|---|---|---|
| `barryvdh/laravel-debugbar` | Uses `register_shutdown_function` | Disable in Octane mode |
| `itsgoingd/clockwork` | Global static state | Use per-request middleware |
| Any package with `static $cache = []` | Cross-request data bleed | Wrap in container binding |

---

## Part 4: Production Rollout

### Canary (recommended)

```bash
# Route 5% traffic to Nusa cluster
# Monitor for 24-48h
# Gradually increase to 100%
```

### Blue/Green (zero downtime)

```bash
# Deploy Nusa alongside FPM
./target/release/nusa --config config.toml &

# Switch load balancer
# Verify health checks
# Remove FPM instances
```

### Rollback

```bash
# If issues detected:
./target/release/nusa rollback

# Revert to FPM
systemctl restart php-fpm nginx
```

---

## Part 5: Performance Tuning

### Worker Sizing

```
workers = min(CPU cores × 2, 16)
memory per worker = max_memory_mb
total RAM needed = workers × memory_per_worker + 256MB (Rust overhead)
```

### Memory-Based Recycling

```toml
# config.toml
max_memory_mb = 512    # recycle at 512MB RSS
max_requests = 1000     # recycle after 1000 requests
timeout_ms = 30000      # 30s request timeout
```

### Benchmark Targets

| Metric | FPM Baseline | Nusa Target |
|--------|-------------|-------------|
| P50 latency | X ms | ≤ X ms |
| P99 latency | X + 50ms | ≤ X + 30ms |
| Throughput | Y RPS | ≥ 1.5× Y RPS |
| Memory stability | variable | ΔRSS ≤ 5% over 50k requests |
