# Production status

This page describes the current maturity level of Nusa and lists remaining gaps before v1.0 GA.

**Workspace version:** `0.1.0` (pre-GA)

---

## Executive summary

| Question | Answer |
|----------|--------|
| Can I pilot Nusa in staging? | **Yes**, on Linux/Alpine; validated by `just podman-ci` and `just podman-ci-e2e` |
| Can I run it in production today? | **Not yet** — benchmark baselines and signed release process still open |
| Embed backend (`octane_backend = "embed"`)? | **Experimental** — stdio daemon via `just podman-ci-embed`; libphp in-process on roadmap |
| Authoritative CI gate? | `just podman-ci` (pre-merge), `just podman-ci-e2e` (pre-GA), `just podman-ci-embed` (embed) on Alpine musl |

---

## What works

### Gateway features

- **Health and readiness** (`/health`, `/ready`) for orchestrators
- **Prometheus metrics** (`/metrics`)
- **Static file serving** with LRU cache, MIME detection, and precompressed `.br`/`.gz` support
- **WebSocket and SSE** managers
- **Async task API** (`/api/tasks`)
- **Global and per-tenant circuit breakers** and **rate limiting**
- **Request size limits**, timeouts, and **backpressure**
- **W3C TraceContext** extraction
- **Tenant routing** from `X-Tenant-Id` and host subdomains

### PHP execution

| Mode | Use |
|------|-----|
| **Octane** (`octane_workers > 0`) | Production Laravel — worker pool (`ipc` or `embed` backend) |
| **Normal** (`octane_workers = 0`) | Debug / migration only — `PhpEngine::execute` (child fork per request) |

Default config: `octane_workers = 4`, `octane_backend = "ipc"`. See [RFC: Native Laravel runtime](../contributor/rfc/nusa-native-laravel-runtime.md).

### Security

- Landlock and seccomp enforced before the server binds
- CI enforcement tests that fail if sandbox cannot be verified
- No "skip on error" pattern

### Octane subsystem

- **Worker pool** with fail-closed `initialize()` when `octane_workers > 0`
- **Gateway dispatch** via `LaravelHttpRuntime` (`ipc` or `embed`) when `is_ready()`
- **Transport**: framed IPC JSON (`ipc`) or NEB1 binary frames (`embed` default)
- **State reset** events on RequestReceived / RequestTerminated
- **PHP driver** (`nusa/octane`, `NusaOctaneServiceProvider`, `nusa-octane-worker`) + Laravel minimal fixture E2E

---

## Known gaps

| ID | Gap | Phase |
|----|-----|-------|
| G1 | FPM baseline benchmark on same host | P3 |
| G2 | Signed release / SBOM | P4 |
| G3 | libphp ZTS in-process production image | P4 |
| G4 | Production async I/O (PDO proxy with bindings, writes) | P4-D |
| G5 | Blueprint "Phase 6" advanced features (ESI, E2E) | P5 |

Addressed items: Octane HTTP dispatch, `/ready` fail-closed, CLI startup validation, Laravel fixture E2E, session/middleware E2E, IPC Cookie parsing, Landlock RW for `storage/`, config-driven tenant registry.

---

## Milestone scorecard

| Milestone | Theme | Status |
|-----------|-------|--------|
| **M0** | Philosophy, plugin model, workspace governance | **Complete** |
| **M1** | Normal mode gateway, engines, config, telemetry | **Complete** |
| **M2** | Octane core, IPC, worker recycle | **Complete in CI** |
| **M3** | TLS, QUIC, ACME modules, WASM engine paths | **Experimental** |
| **M4** | Multi-tenant, tasks, plugins | **Complete** |
| **M5** | Benchmarks, release matrix, GA | **Partial** — P3–P4 open |

---

## Engine matrix

| Engine | Role | Production today |
|--------|------|------------------|
| **`child`** | PHP subprocess isolation | **Yes** — default, tested Normal mode |
| **`ffi`** | Embedded PHP (ZTS) on Linux | **Conditional** — requires compatible PHP build |
| **`wasm`** | Sandboxed PHP (future) | **No** — CLI uses stub |

---

## How we verify releases

```bash
just podman-build      # when image inputs change
just podman-ci-fast    # fmt + lint + workspace tests
just podman-ci         # pre-merge: workspace + Laravel E2E — authoritative
just podman-ci-e2e     # pre-GA: + leak 10k + IPC bench smoke
```

Host `just ci` is for local iteration only — **not** merge sign-off.

### Latest Alpine sign-off

| Recipe | Result | Notes |
|--------|--------|-------|
| `just podman-ci-fast` | **Pass** | 1620 workspace tests |
| `just podman-ci` | **Pass** | + Laravel live E2E |
| `just podman-ci-e2e` | **Pass** | + `octane_leak_suite` 10k requests + `ipc_latency_bench` smoke |

Benchmark data: [`docs/benchmarks/normal-mode-report.md`](../benchmarks/normal-mode-report.md).

---

## Next steps

- **Developers:** [Laravel documentation](laravel/README.md)
- **Operators:** [Operations runbook](operations/runbook.md)
- **Contributors:** [Contributor docs](../contributor/README.md)
- **RFC roadmap:** [Native Laravel runtime](../contributor/rfc/nusa-native-laravel-runtime.md)
