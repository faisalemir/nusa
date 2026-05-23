# Default tasks for Nusa PHP Runtime development.
#
# Install: https://github.com/casey/just
# Usage: just <task>

# Use PowerShell on Windows
set shell := ["powershell.exe", "-c"]

# --- Build & Test ---

# Workspace members that need native PHP/ZTS (skip on host; run in podman-ci)
workspace-test-excludes := "--exclude nusa-engine-ffi"

# Warm registry dependency artifacts (re-run after Cargo.lock changes).
# External crates from crates.io are cached; workspace members (path = "...") always
# recompile when their .rs sources change — that is expected, not a version bump.
cache-deps:
    cargo build --workspace {{workspace-test-excludes}} --lib --bins --tests

# Run all tests with nextest (full rebuild — slow)
test:
    cargo nextest run --workspace {{workspace-test-excludes}}

# Run all tests; continue after failures (diagnostics)
test-all:
    cargo nextest run --workspace {{workspace-test-excludes}} --no-fail-fast

# Quick test — reuse compiled test binaries when sources unchanged
test-fast:
    cargo nextest run --workspace {{workspace-test-excludes}} --no-fail-fast --retries 0

# Run only new+extended test files (gap-analysis additions)
test-new:
    cargo nextest run --workspace --no-fail-fast -E "binary(~_exhaustive) | binary(~_extended) | binary(~_domain) | binary(~_e2e) | binary(~stress_decision) | binary(lifecycle_test) | binary(security_enforcement_test) | binary(concurrency_resource_test) | binary(test_runner_test) | binary(soak_test)"

# Run tests for a specific crate (fast — single crate)
test-crate CRATE:
    cargo nextest run --workspace -p nusa-{{CRATE}}

# Run tests with coverage
test-coverage:
    cargo llvm-cov nextest --workspace --lcov > coverage.lcov

# Run all Criterion benchmarks (release)
bench:
    cargo bench -p nusa-benchmarks

# Run a single benchmark (e.g. just bench-fast gateway_bench)
bench-fast BENCH:
    cargo bench -p nusa-benchmarks --bench {{BENCH}}

# Run clippy with deny warnings (libs + integration tests)
lint:
    cargo clippy --workspace {{workspace-test-excludes}} --tests --bins -- -D warnings

# Format all code
fmt:
    cargo fmt --all

# Check formatting without changing files
fmt-check:
    cargo fmt --all -- --check

# Build release binary
build-release:
    cargo build --release

# Build for Linux musl (static binary)
build-musl:
    cargo build --release --target x86_64-unknown-linux-musl

# --- Security ---

# Audit dependencies for known vulnerabilities
audit:
    cargo audit

# Check for unsafe code usage
geiger:
    cargo geiger

# Check license compliance
deny:
    cargo deny check

# Run all security checks
security: audit geiger deny

# --- CI ---

# Full local gate: format, lint, compile cache, tests (zero errors/warnings)
ci-local: fmt-check lint cache-deps test security

# Run full CI pipeline locally (host OS — not authoritative for merge; use podman-ci)
ci: fmt-check lint test security

# --- Podman Alpine ---
#
# Tiers (production target = Alpine musl; image must exist — run `just podman-build` when
# Dockerfile, Cargo.lock, or Laravel fixture deps change):
#   podman-ci-fast     — fmt + lint + workspace tests (~5–10 min typical)
#   podman-ci          — fast + Laravel E2E smoke (pre-merge)
#   podman-ci-e2e      — ci + leak 10k + IPC bench (pre-GA / nightly)
#   podman-ci-rebuild  — rebuild image then podman-ci
#
# Image build (just podman-build):
#   Layer 1: cargo-chef — crates.io deps → /opt/nusa-target
#   Layer 2: full source + `nextest --no-run` — test binaries precompiled in image
#
# Live mount: host /src only; CARGO_TARGET_DIR=/opt/nusa-target from image.
# Laravel fixture: dockerfiles/podman-laravel-fixture.sh skips composer when vendor/ is valid.

# Build test image (~10 min first time; ~1–3 min when only sources change)
podman-build:
    podman build -t nusa-test-runner -f dockerfiles/nusa-test-runner.Dockerfile .

# Fail fast if the test image is missing (avoids implicit rebuild every CI run)
podman-require-image:
    @podman image exists nusa-test-runner; if ($LASTEXITCODE -ne 0) { Write-Error 'Missing image nusa-test-runner. Run: just podman-build'; exit 1 }

# Mount live source; never use host target/ (avoids file-lock on Windows).
podman-run-mount := '-v "' + justfile_directory() + ':/src:Z" -w /src'

# Shell prefix: musl flags consistent with the image (stable fingerprints vs /opt/nusa-target).
podman-cargo-sh := 'cp -f .cargo/config-alpine.toml .cargo/config.toml && '

# Conditional composer + fixture dirs (see dockerfiles/podman-laravel-fixture.sh)
podman-laravel-fixture-sh := 'sh dockerfiles/podman-laravel-fixture.sh && '

# Workspace tests only (live source; no nusa-e2e-tests)
podman-test-workspace:
    podman run --rm -t {{podman-run-mount}} nusa-test-runner sh -c "{{podman-cargo-sh}} cargo nextest run --workspace {{workspace-test-excludes}} --exclude nusa-cli --exclude nusa-e2e-tests --test-threads 4"

# Laravel / Octane E2E package only (serial)
podman-test-e2e:
    podman run --rm -t -e NUSA_LARAVEL_FIXTURE=/src/tests/fixtures/laravel-minimal {{podman-run-mount}} nusa-test-runner sh -c "{{podman-cargo-sh}}{{podman-laravel-fixture-sh}} cargo nextest run -p nusa-e2e-tests --test-threads 1"

# Full workspace in Alpine (reuses /opt/nusa-target from image; recompile only changed crates).
podman-test-full:
    podman run --rm -t {{podman-run-mount}} nusa-test-runner sh -c "{{podman-cargo-sh}} cargo nextest run --workspace --test-threads 4 --no-fail-fast"

# Live source: workspace + E2E (same as podman-ci test phase, without fmt/lint/build)
podman-test-live: podman-test-workspace podman-test-e2e

# P2: Laravel fixture E2E only (alias)
podman-test-laravel: podman-test-e2e

# CI subset in Alpine (image-baked source; does NOT validate host working tree)
podman-test:
    podman run --rm -t nusa-test-runner cargo nextest run --workspace {{workspace-test-excludes}} --exclude nusa-cli --test-threads 4

# Run specific test crate (example: just podman-test-pkg gateway)
podman-test-pkg PACKAGE:
    podman run --rm -t {{podman-run-mount}} nusa-test-runner sh -c "{{podman-cargo-sh}} cargo nextest run -p nusa-{{PACKAGE}} --test-threads 4"

# Run clippy in Alpine (live source)
podman-lint:
    podman run --rm -t {{podman-run-mount}} nusa-test-runner sh -c "{{podman-cargo-sh}} cargo clippy --workspace --tests --bins -- -D warnings"

# Octane leak suite only (no duplicate laravel_live / trace runs)
podman-test-laravel-leak:
    podman run --rm -t -e NUSA_LARAVEL_FIXTURE=/src/tests/fixtures/laravel-minimal -e NUSA_LEAK_REQUESTS=10000 {{podman-run-mount}} nusa-test-runner sh -c "{{podman-cargo-sh}}{{podman-laravel-fixture-sh}} sh tests/fixtures/laravel-minimal/scripts/artisan-octane-contract.sh && cargo nextest run -p nusa-e2e-tests -E 'binary(octane_leak_suite)' --test-threads 1"

# Faster leak dev run (override: just podman-test-laravel-leak-dev)
podman-test-laravel-leak-dev:
    podman run --rm -t -e NUSA_LARAVEL_FIXTURE=/src/tests/fixtures/laravel-minimal -e NUSA_LEAK_REQUESTS=1000 {{podman-run-mount}} nusa-test-runner sh -c "{{podman-cargo-sh}}{{podman-laravel-fixture-sh}} cargo nextest run -p nusa-e2e-tests -E 'binary(octane_leak_suite)' --test-threads 1"

# IPC latency bench in Alpine (P2 KPI: inspect P99 in output; target <= 10ms release)
podman-bench-ipc-smoke:
    podman run --rm -t {{podman-run-mount}} nusa-test-runner sh -c "{{podman-cargo-sh}} cargo bench -p nusa-benchmarks --bench ipc_latency_bench -- --noplot"

# Pre-merge (typical): no image rebuild; ~5–15 min depending on changed crates
podman-ci-fast: fmt-check podman-require-image podman-lint podman-test-workspace

# Pre-merge authoritative: musl workspace + Laravel E2E smoke
podman-ci: fmt-check podman-require-image podman-lint podman-test-live

# Pre-GA / nightly: workspace + E2E + leak 10k + IPC bench (no duplicate full e2e pass)
podman-ci-e2e: fmt-check podman-require-image podman-lint podman-test-workspace podman-test-e2e podman-test-laravel-leak podman-bench-ipc-smoke

# Rebuild image then run pre-merge gate (Dockerfile / Cargo.lock / fixture deps changed)
podman-ci-rebuild: podman-build podman-ci

# Rebuild image from scratch (clears image layers)
podman-clean:
    podman rmi nusa-test-runner 2>/dev/null || true

# Remove legacy Podman volumes (no longer used; target/registry live in the image)
podman-clean-cache:
    podman volume rm nusa-cargo-registry nusa-cargo-git nusa-target-cache nusa-cargo-cache 2>/dev/null || true

# --- Docker ---

# Build Docker image
docker-build:
    docker build -t nusa .

# Run container
docker-run:
    docker run -p 8080:8080 nusa

# --- Docs ---

# Generate API documentation
docs:
    cargo doc --workspace --no-deps

# Open generated docs in browser
docs-open:
    cargo doc --workspace --no-deps --open

# --- Cleanup ---

# Clean build artifacts
clean:
    cargo clean
