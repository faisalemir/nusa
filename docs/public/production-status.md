# Production status

This page is the contract between the Nusa team and everyone who deploys Laravel on the runtime. We describe **what is already exceptional**, **what is intentionally incomplete**, and **how we gate quality**—without pretending v0.1.0 is a finished v1.0 GA release.

**Workspace version:** `0.1.0` (pre-GA)

---

## Executive summary

| Question | Answer |
|----------|--------|
| Is Nusa a credible platform engineering bet? | **Yes**—architecture, sandbox, gateway, IPC, and test depth are real |
| Can I run it in production at unlimited scale today? | **Not yet**—finish P1 Octane HTTP dispatch, P2 Laravel E2E, P3–P4 benchmarks and release process |
| Can I pilot in staging / internal platforms? | **Yes**, on Linux/Alpine with eyes open on the gaps below |
| What proves quality? | **`just podman-ci`** on Alpine musl—not host-only green runs |

Nusa is **ahead of typical “0.1” projects** in systems design: kernel sandboxing, framed IPC, multi-tenant gateway controls, and thousands of tests. It is **behind GA** on a narrow set of integration wires (chiefly Octane HTTP → worker pool) and formal release KPIs.

That combination is a strength, not a secret—we document it so you can plan migrations with confidence.

---

## What already works (and why it matters)

### HTTP gateway as a product surface

The `nusa-gateway` crate is a full application edge—not a thin proxy:

- **Health and readiness** (`/health`, `/ready`) for orchestrators
- **Prometheus metrics** (`/metrics`) wired to a real recorder
- **Static file serving** with caching, MIME detection, and traversal hardening
- **WebSocket and SSE** managers for real-time Laravel features
- **Async task API** (`/api/tasks`) to offload work without blocking the request thread
- **Global and per-tenant circuit breakers** plus **rate limiting**
- **Request size limits**, timeouts, and **backpressure** aligned with `max_workers`
- **W3C TraceContext** extraction for distributed traces
- **Tenant routing** from `X-Tenant-Id` and host subdomains

This is the layer you would otherwise assemble from nginx, Envoy, a sidecar, and custom PHP glue.

### PHP execution (Normal mode)

With `engine = "child"` and `octane_workers = 0`, requests flow through `PhpEngine::execute`—the **supported production path today**. Child processes give you strong isolation semantics familiar from FPM, supervised by Rust lifecycle and error mapping to HTTP status codes.

### Security enforcement on Linux

Landlock and seccomp are applied **before** the server binds. CI on Alpine musl includes enforcement tests that **fail** if sandbox application cannot be verified—no “skip on error” pattern. For regulated and multi-tenant environments, that discipline is the difference between checkbox compliance and actual containment.

### Octane subsystem (foundation complete, HTTP wire pending)

The Octane story is substantial even before P1:

- **Worker pool** construction and initialization in the CLI
- **Framed IPC** (`nusa-ipc`) with handshake, heartbeat, and request/response semantics
- **State reset orchestrator** for leak-sensitive long-lived workers
- **PHP driver** entry (`octane-rust-worker`) for Laravel bootstrap

What remains is the **gateway routing decision**: when the pool is ready, HTTP must enter the pool path instead of only `engine.execute`. That is P1—not a redesign.

### Observability and operations

Structured logging, metrics, and probe endpoints mean you can deploy Nusa the same way you deploy Go or Rust microservices: scrape, alert, drain, roll.

### Test corpus

The project maintains a **large, categorized test suite**—security, concurrency, resource exhaustion, decision tables, gateway E2E, IPC stress—run authoritatively in Alpine containers. This is unusual depth for a young runtime and is the basis for our confidence in what we mark “works.”

---

## Known gaps (transparent roadmap)

| ID | Gap | Why it blocks GA | Phase |
|----|-----|------------------|-------|
| G1 | Gateway does not dispatch HTTP to `WorkerPool` when `octane_workers > 0` | Octane mode does not yet deliver its headline latency benefit over HTTP | **P1** |
| G2 | `/ready` does not fail closed when Octane pool is required but unhealthy | Orchestrators may send traffic to a broken worker tier | **P1** |
| G3 | CLI warns and continues if pool init fails while workers > 0 | Silent partial startup is unacceptable for production | **P1** |
| G4 | No default CI Laravel fixture E2E | Need proof on real `vendor/` + `bootstrap/app.php` | **P2** |
| G5 | Normal Mode latency KPIs and signed release process | GA bar is measured, not asserted | **P3–P4** |
| G6 | Blueprint “Phase 6” advanced features | Post-GA innovation track | **P5** |

We do not hide these behind optimistic README tables. [Migration](migration.md) and [Operations](operations/runbook.md) repeat the Octane caveat where it affects your runbooks.

---

## Milestone scorecard

| Milestone | Theme | Status |
|-----------|-------|--------|
| **M0** | Philosophy, plugin model, workspace governance | **Strong** — vision documented; core traits in place |
| **M1** | Normal mode gateway, engines, config hot-reload, telemetry | **Largely complete** — primary path for pilots |
| **M2** | Octane core, IPC, worker recycle | **Partial** — pool and protocol; HTTP dispatch → P1 |
| **M3** | TLS, QUIC, ACME modules, WASM engine paths | **Partial** — code present; environment-dependent activation |
| **M4** | Multi-tenant, tasks, plugins | **Partial** — gateway hooks and core types wired |
| **M5** | Benchmarks, release matrix, GA | **Planned** |

---

## Engine matrix (today)

| Engine | Role | Production today |
|--------|------|------------------|
| **`child`** | PHP subprocess isolation | **Yes** — default, tested Normal mode |
| **`ffi`** | Embedded PHP (ZTS) on Linux | **Conditional** — requires compatible PHP build |
| **`wasm`** | Sandboxed PHP (future) | **No** — CLI uses stub; not real PHP execution yet |

Choose `child` unless you have a deliberate FFI validation program.

---

## How we verify releases

```bash
just podman-build      # Alpine test image
just podman-ci         # fmt + clippy (-D warnings) + tests — authoritative
just podman-test-full  # full workspace run with logged results
```

Host `just ci` helps developers on Windows or macOS iterate; it is **not** merge sign-off for production integrity.

---

## Roadmap pointer

Execution phases **P0–P5** are tracked in [contributor RFC](../contributor/rfc/production-readiness.md). Documentation you are reading is **P0**; Octane HTTP wiring is **P1**.

When Phase 5 completes, this page will flip from “pre-GA with listed gaps” to “GA criteria met”—with benchmark numbers and release artifacts linked, not adjectives.

---

## Closing perspective

Nusa is built for teams who believe **Laravel’s developer experience should not cost kernel-level naïveté**. The runtime is young on the calendar and **advanced in the engineering that survives contact with production**.

Use it today to **learn, pilot, and harden your platform**; use this page to know exactly when to **bet the business**. We prefer that honesty over a premature “GA” badge—and we think you will too.
