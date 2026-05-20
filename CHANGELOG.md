# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Phase 0: Foundation — workspace structure, core traits, error model, config, governance
- Phase 1: Normal Mode MVP — Axum gateway, resource guards, circuit breaker, health probes, TLS config
- Phase 2: Octane Core — Worker pool manager, IPC protocol, state reset orchestrator, PHP driver package
- Phase 3: WASM sandbox — wasmtime engine with memory/fuel limits, trap-to-error conversion
- Security hardening — Landlock FS rules, Seccomp syscall filter stubs
- Observability — OpenTelemetry tracing, Prometheus metrics, structured JSON logs
- 130 integration tests covering core types, IPC framing, gateway, worker pool, WASM engine
- Dockerfile with PHP ZTS build stage and Alpine runtime
- CI pipeline (GitHub Actions) with fmt, clippy, audit, deny checks
- cargo-nextest integration for parallel test execution (2.6s for 130 tests)

### Security
- `#![deny(unsafe_code)]` enforced in all crates except `nusa-engine-ffi`
- All unsafe blocks in FFI crate documented with `// SAFETY:` comments
- Zero CVEs found in 452+ dependencies

### Changed
- Updated all dependencies to latest versions (opentelemetry 0.32, wasmtime 44, notify 8.2)
- Rust edition 2024, rust-version 1.95

[Unreleased]: https://github.com/nusa-rs/nusa/compare/v0.1.0...HEAD
