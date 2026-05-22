# Default tasks for Nusa PHP Runtime development.
#
# Install: https://github.com/casey/just
# Usage: just <task>

# Use PowerShell on Windows
set shell := ["powershell.exe", "-c"]

# --- Build & Test ---

# Run all tests with nextest (full rebuild — slow)
test:
    cargo nextest run --workspace

# Quick test — only recompile changed tests (fast)
test-fast:
    # Force nextest to use existing binaries, skip recompilation
    CARGO_INCREMENTAL=0 cargo nextest run --workspace --no-fail-fast --retries 0

# Run only new+extended test files (fast — limited scope)
test-new:
    cargo nextest run --workspace --test security_exhaustive_test --test fallback_test --test state_restore_test --test stress_test --test error_propagation_test --test soak_test --test property_test --test platform_test --test decision_table_test

# Run tests for a specific crate (fast — single crate)
test-crate CRATE:
    cargo nextest run --workspace -p nusa-{{CRATE}}

# Run tests with coverage
test-coverage:
    cargo llvm-cov nextest --workspace --lcov > coverage.lcov

# Run clippy with deny warnings
lint:
    cargo clippy --workspace -- -D warnings

# Format all code
fmt:
    cargo fmt --workspace

# Check formatting without changing files
fmt-check:
    cargo fmt --workspace --check

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

# Run full CI pipeline locally
ci: fmt-check lint test security

# --- Podman Alpine (Best Practices) ---
#
# Strategy: COPY source into image during build (no volume mount).
# Dockerfile uses 2-layer approach for fast rebuilds:
#   Layer 1: Cargo manifests + cargo build --lib (cached if deps unchanged)
#   Layer 2: Full source + test execution
#
# Benefits:
#   - No Windows → WSL volume mount overhead
#   - Deps cached in image layer (fast rebuild ~30s)
#   - Test runs on native ext4 filesystem
#   - mold linker + CARGO_TERM_COLOR for progress

# Build test image (first time: ~10 min, subsequent: ~30s)
podman-build:
    podman build -t nusa-test-runner -f dockerfiles/nusa-test-runner.Dockerfile .

# Run all workspace tests (excludes nusa-engine-ffi — needs PHP ZTS headers)
podman-test:
    podman run --rm -t nusa-test-runner cargo nextest run --workspace --exclude nusa-engine-ffi --exclude nusa-cli --test-threads 4

# Run specific test crate (example: just podman-test-pkg -p nusa-gateway)
podman-test-pkg PACKAGE:
    podman run --rm -t nusa-test-runner cargo nextest run --workspace --test-threads 4 -p {{PACKAGE}}

# Run clippy in Alpine
podman-lint:
    podman run --rm -t nusa-test-runner cargo clippy --workspace -- -D warnings

# Run full CI in Alpine
podman-ci: podman-build podman-lint podman-test

# Rebuild image from scratch (clears cache)
podman-clean:
    podman rmi nusa-test-runner 2>/dev/null || true

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
