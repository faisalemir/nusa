# Default tasks for Nusa PHP Runtime development.
#
# Install: https://github.com/casey/just
# Usage: just <task>

# --- Build & Test ---

# Run all tests with nextest (parallel, fast)
test:
    cargo nextest run --workspace

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
