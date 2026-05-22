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
# Image build (just podman-build):
#   Layer 1: cargo-chef — crates.io deps → /opt/nusa-target
#   Layer 2: full source + `nextest --no-run` — all test binaries precompiled in image
#
# Live mount (podman-test-full): host /src only; CARGO_TARGET_DIR stays /opt/nusa-target
# from the image (no target volume — a volume would hide prebuilt artifacts).
# Only workspace crates recompile when you change local .rs files.

# Build test image (~10 min first time; ~1–3 min when only sources change)
podman-build:
    podman build -t nusa-test-runner -f dockerfiles/nusa-test-runner.Dockerfile .

# Mount live source; never use host target/ (avoids file-lock on Windows).
podman-run-mount := '-v "' + justfile_directory() + ':/src:Z" -w /src'

# Shell prefix: musl flags consistent with the image (stable fingerprints vs /opt/nusa-target).
podman-cargo-sh := 'cp -f .cargo/config-alpine.toml .cargo/config.toml && '

# Full workspace in Alpine (reuses /opt/nusa-target from image; recompile only changed crates).
podman-test-full:
    podman run --rm -t {{podman-run-mount}} nusa-test-runner sh -c "{{podman-cargo-sh}} cargo nextest run --workspace --test-threads 4 --no-fail-fast"

# CI subset with live source (no FFI on host; FFI stub OK in Alpine).
podman-test-live:
    podman run --rm -t {{podman-run-mount}} nusa-test-runner sh -c "{{podman-cargo-sh}} cargo nextest run --workspace {{workspace-test-excludes}} --exclude nusa-cli --test-threads 4"

# CI subset in Alpine (image-baked source; faster when not validating local edits).
podman-test:
    podman run --rm -t nusa-test-runner cargo nextest run --workspace {{workspace-test-excludes}} --exclude nusa-cli --test-threads 4

# Run specific test crate (example: just podman-test-pkg gateway)
podman-test-pkg PACKAGE:
    podman run --rm -t {{podman-run-mount}} nusa-test-runner sh -c "{{podman-cargo-sh}} cargo nextest run -p nusa-{{PACKAGE}} --test-threads 4"

# Run clippy in Alpine (live source)
podman-lint:
    podman run --rm -t {{podman-run-mount}} nusa-test-runner sh -c "{{podman-cargo-sh}} cargo clippy --workspace --tests --bins -- -D warnings"

# Run full CI in Alpine
podman-ci: podman-build podman-lint podman-test-live

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
