# Nusa PHP Runtime

A modern, Rust-orchestrated PHP runtime for Laravel: FPM-compatible out-of-the-box, Octane-optimized by design, cloud-native, memory-safe, and fully observable.

> *"Where Rust's speed meets Laravel's soul, in an architecture that is secure, scalable, and sovereign."*

## Architecture

```
Client → TLS/HTTP2 → Rust Gateway (Axum)
                      ├─ Middleware Chain (Trace, CORS, Circuit Breaker, Backpressure)
                      ├─ Normal Mode: Route → PHP Engine (FFI/WASM/Process) → Response
                      └─ Octane Mode: Route → Worker Pool → IPC → Laravel Worker → Response
                      │
                      ├─ Security: Landlock, Seccomp-BPF, VFS Guard
                      ├─ Observability: OTLP Tracing, Prometheus Metrics, JSON Logs
                      ├─ Config: TOML + Env, Hot-Reload (ArcSwap)
                      └─ Plugin API: WASM sandbox, versioned hooks
```

### Crate Structure

| Crate | Purpose | Milestone |
|-------|---------|-----------|
| `phprt-core` | Core types, traits, errors, resource guards | M0, M1 |
| `phprt-gateway` | Axum HTTP server, middleware, health probes, TLS | M1, M3 |
| `phprt-engine-ffi` | PHP ZTS FFI wrapper (isolated unsafe) | M1 |
| `phprt-engine-wasm` | PHP WASM sandbox (wasmtime) | M3 |
| `phprt-engine-child` | PHP child process engine | M1, M2 |
| `phprt-ipc` | IPC Contract v1: framed binary codec | M2 |
| `phprt-octane-worker` | Worker pool manager with recycle logic | M2 |
| `phprt-config` | TOML/Env config with ArcSwap hot-reload | M0 |
| `phprt-security` | Landlock + Seccomp hardening | M3 |
| `phprt-telemetry` | OpenTelemetry + Prometheus | M1, M3 |
| `phprt-plugin-api` | Plugin registry with pre/post exec hooks | M0, M2 |
| `phprt-cli` | Binary entry point (phprt) | M0, M1 |

## Phase 1 Status: ✅ Complete

| Feature | Status | Tests |
|---------|--------|-------|
| Resource guards (backpressure, timeout, size limit) | ✅ | 58 tests pass |
| Health probes (/health, /ready) | ✅ | Gateway integration tests |
| Full HTTP handling (RequestContext builder) | ✅ | Core + gateway tests |
| TLS 1.3 configuration | ✅ | Rustls-based TlsConfig |
| Circuit breaker | ✅ | 9 dedicated tests |
| FFI engine (Linux conditional) | ✅ | Stub for Windows, bindgen for Linux |
| PHP driver package | ✅ | composer.json + worker script |
| Dockerfile (Alpine musl, PHP ZTS) | ✅ | Multi-stage build |
| Config hot-reload | ✅ | 6 config tests |
| W3C TraceContext extraction | ✅ | Gateway implementation |
| Multi-tenant support | ✅ | TenantId extraction from headers |

### Build Status

| Check | Status |
|-------|--------|
| `cargo check --workspace` | ✅ Clean |
| `cargo clippy --workspace -- -D warnings` | ✅ 0 warnings |
| `cargo test --workspace` | ✅ 58/58 pass |
| `cargo audit` | ✅ 0 CVEs |

## Quick Start

### Prerequisites

- Rust 1.95+
- PHP 8.3+ (for child process engine)
- Optional: PHP ZTS with --enable-embed (for FFI engine)

### Run

```bash
# Copy example config
cp config.toml.example config.toml

# Build
cargo build --release

# Run
./target/release/phprt --config config.toml
```

### Docker

```bash
docker build -t phprt .
docker run -p 8080:8080 -v /path/to/laravel:/app/public phprt
```

### Verify

```bash
curl http://localhost:8080/health   # → "OK"
curl http://localhost:8080/ready    # → "READY" (after initialization)
```

## Configuration

Copy `config.toml.example` to `config.toml` and adjust:

```toml
engine = "child"        # "ffi" | "wasm" | "child"
max_workers = 4
timeout_ms = 30000
vfs_root = "/app/public"
code_dir = "/app/public"
tmp_dir = "/tmp/phprt"
hot_reload = true
```

Environment variables override config values (prefix with `PHPRT_`):

```bash
export PHPRT_MAX_WORKERS=8
export PHPRT_TIMEOUT_MS=60000
```

## Milestones

| Phase | Scope | Status |
|-------|-------|--------|
| **0: Foundation** | Workspace, core traits, error model, CI/CD, governance | ✅ Complete |
| **1: Normal Mode** | Gateway, engines, TLS, resource guards, health probes | ✅ Complete |
| **2: Octane Core** | Worker pool, IPC protocol, Laravel driver | ⏳ Skeleton |
| **3: Security** | Landlock, Seccomp, WASM sandbox, circuit breaker | ✅ Partial |
| **4: Advanced** | Async task offload, telemetry-driven recycle | ⏳ Skeleton |
| **5: Ecosystem & GA** | Composer package, benchmarks, SLSA | ⏳ Planned |

## Security

- `#![deny(unsafe_code)]` enforced in all crates except `phprt-engine-ffi`
- All unsafe blocks in FFI crate have `// SAFETY:` comments
- Zero CVEs found in 452+ dependencies
- SBOM generation and SLSA provenance in CI pipeline

See [SECURITY.md](SECURITY.md) and [docs/security/threat-model.md](docs/security/threat-model.md).

## Contributing

This project follows a strict meta-cognitive framework:

1. **Layer 3 (WHY):** Solve real Laravel/Cloud-Native problems
2. **Layer 2 (WHAT):** Sound architecture, modular, safe
3. **Layer 1 (HOW):** Rust idioms (ownership, concurrency, errors)

See [CONTRIBUTING.md](CONTRIBUTING.md) for RFC process and code standards.

## License

MIT
