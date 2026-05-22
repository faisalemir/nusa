# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

#### Documentation (P0)
- Greenfield docs: `docs/public/`, `docs/contributor/`, `docs/ai/`, `docs/README.md`
- `AGENTS.md` entry point for AI agents
- Honest [production status](docs/public/production-status.md) (v0.1.0 pre-GA)
- Expanded public narrative: vision, architecture value, migration story (still accurate on Octane P1 gap)

### Changed
- `README.md` no longer claims Phase 1–4 complete or fixed 58-test count
- `config.toml.example` documents Octane keys (`octane_workers`, recycle limits)
- Migration and runbook moved under `docs/public/`

### Known gaps (documented, not fixed in this release)
- Gateway HTTP handler still calls `engine.execute` when Octane pool is configured (P1)
- `/ready` does not fail closed on Octane pool init failure (P1)

#### Phase 1: Normal Mode MVP (historical changelog — verify against code)
- Static file serving wired — `/static/{*path}` routes to `StaticFileHandler` with LRU cache, MIME types, Cache-Control headers, directory traversal prevention
- Prometheus metrics endpoint — `/metrics` now renders full Prometheus exposition format from global recorder
- `NusaMetrics` and `PrometheusHandle` wired into `AppState` for shared access

#### Phase 2: Octane Core
- Child engine fully wired — PHP child processes communicate via framed IPC over stdin/stdout (`IpcMessage::Request`/`Response`)
- Worker handshake complete — orchestrator sends `Hello`, awaits `Ack` with 5s timeout, handles all error cases
- WorkerPool wired into gateway — `AppState` includes `Arc<Mutex<Option<WorkerPool>>>` and `StateResetOrchestrator`
- IPC transport layer complete — framing codec, Unix socket connect/send/recv, request_response, heartbeat

#### Phase 3: Security
- WASM sandbox fully wired — `WasmRuntime` integrated into `WasmEngine::execute()`, `StoreLimitsBuilder` enforces memory caps, `Config::consume_fuel(true)` + `set_fuel()` enforce per-request fuel limits, fuel exhaustion mapped to `EngineError::ResourceLimit`, WASM traps mapped to `EngineError::Sandbox`
- Seccomp-BPF syscall filter — pure-Rust `seccompiler` crate with ~70 syscall whitelist (I/O, memory, network, threading, file operations)
- Landlock WRITE access for `tmp_dir` — separate ruleset with `WRITE_FILE`, `READ_FILE`, `READ_DIR`, `MAKE_FILE`, `REMOVE_FILE`

#### Phase 4: Advanced Features
- WebSocket upgrade handler — full WebSocket support via `axum` `ws` feature, `WsManager` with connection lifecycle, broadcast to tenant, ping/pong heartbeat, binary message support
- Redis pub/sub broadcast bridge — `BroadcastBridge::run_listener()` subscribes to channels, forwards messages to `WsManager` and `SseManager`
- Dev watcher debounce — `DevWatcher` uses tokio timer batching to collect file events within `debounce_ms` window before handling
- Test runner — `TestRunner` enumerates test files, dispatches to worker pool via IPC, collects results, falls back to PHPUnit/Pest
- State reset orchestrator — proper stats tracking with separate counters for requests/resets/cleanups/stops

### Changed
- `WasmEngine` constructor accepts WASM path, memory limit, and fuel per request
- `ChildEngine` constructor accepts both `php_binary` and `bootstrap_script` paths
- `BroadcastBridge::run_listener()` now takes `ws_manager` and `sse_manager` references for message forwarding
- `StaticFileHandler::serve()` simplified to not require `Request` parameter

### Fixed
- Removed unused `Command` import in `pool.rs` that caused compilation warnings
- WebSocket handler compilation errors (type mismatches with `SplitSink`/`SplitStream`)

### Security
- `#![deny(unsafe_code)]` enforced in all crates except `nusa-engine-ffi`
- All unsafe blocks in FFI crate documented with `// SAFETY:` comments
- Zero CVEs found in 452+ dependencies
- Added `seccompiler` 0.5 and `landlock` 0.4 dependencies for Linux syscall/filesystem sandboxing

[Unreleased]: https://github.com/nusa-rs/nusa/compare/v0.1.0...HEAD
