# RFC: Nusa Native Laravel Runtime

**Status:** Accepted (implementation in progress)  
**Version:** 2026-05-23  
**Workspace:** Nusa v0.1.x → v0.2+  
**PHP:** 8.5.6 (Alpine `php85`, CI-pinned)

## Summary

`nusa` becomes a **single-process Laravel runtime**: Rust gateway + sandbox + metrics, with Laravel executed via long-lived PHP workers. Production target is **libphp ZTS embedded in-process** (FrankenPHP-class). Until embed images ship, **`octane_backend = ipc`** remains the default CI path; **`embed`** uses the PHP embed bridge (stdio daemon or FFI when `embed-php` is enabled).

## Goals

- One deployable binary (`nusa`) for Laravel — no nginx/php-fpm/RoadRunner/Bun/Go alongside.
- Octane semantics: bootstrap Laravel once per worker, many requests.
- KPI vs **php-fpm / RoadRunner / FrankenPHP** — not Bun/Go hello-world.

## Non-goals

- Rewriting PHP in Rust.
- Matching Bun RPS on full Laravel routes.
- Removing IPC backend before v1.0.

## Architecture

| Backend | `octane_backend` | Mechanism |
|---------|------------------|-----------|
| IPC (default v0.2) | `ipc` | `php nusa-octane-worker` + UDS + JSON |
| Embed | `embed` | In-process FFI when available; else stdio embed daemon |
| Normal | `octane_workers = 0` | `ChildEngine` — debug/migration only |

## Core APIs

- **`LaravelHttpRuntime`** (`nusa-core`) — `handle_http_request`, `is_ready`, `recycle_workers`, `shutdown`.
- **`FfiWorkerPool`** (`nusa-engine-embed`) — embed backend.
- **`WorkerPool`** — implements trait for IPC.

## Config

```toml
octane_workers = 4
octane_backend = "ipc"   # or "embed"
```

Env: `NUSA_OCTANE_WORKERS`, `NUSA_OCTANE_BACKEND`.

## Migration

| Release | Default |
|---------|---------|
| 0.1.x | `octane_workers=0`, IPC only |
| 0.2.0 | Examples/init use `octane_workers=4`; backend `ipc`; embed experimental |
| 0.3.0+ | Production image prefers `embed` |

## KPI (v1.0.0)

- K1–K2: S2-embed P50/P99 vs FrankenPHP.
- K3: Beat php-fpm on equivalent routes.
- K4: S0 gateway ≥ 10⁴ RPS smoke.
- K5: `just podman-ci` + `just podman-ci-embed` green.
- K6: Runbook — single container, only `nusa`.

## Phase 2 gate (implementation)

| Check | Status |
|-------|--------|
| `LaravelHttpRuntime` + gateway `laravel_runtime` | Done |
| `Embed/Runtime.php`, `nusa_embed_daemon.php` | Done |
| `nusa-engine-embed` / `FfiWorkerPool` (stdio; FFI later) | Done |
| `octane_backend`, `init_laravel_runtime()` | Done |
| `just podman-ci-embed` | Done |
| `laravel_embed_pool_ping` + session round-trip | Done (Alpine) |
| S2-embed bench row | Done (smoke script) |
| libphp ZTS production image | Roadmap (see [embed-runtime-image.md](../embed-runtime-image.md)) |

## Phase 4 — Performance envelope (OOTB around libphp)

libphp/Zend cannot match compiled runtimes (Go/Rust/Bun) on raw interpreter throughput. Nusa does **not** rewrite the engine; it compresses **perceived latency** by making boot, I/O, transport, and static work cheap in Rust.

**Performance formula (product):**

```text
Rust (gateway + static + async I/O + warm workers) + Laravel (embed/IPC) ≈ modern perceived latency
```

Fair benchmarks remain **FrankenPHP / RoadRunner / php-fpm**, not Bun hello-world.

### Strategies (out-of-the-box)

| ID | Strategy | Problem addressed | Nusa today | Target phase |
|----|----------|-------------------|------------|--------------|
| **P4-A** | **Shadow / warm standby pool** | Laravel bootstrap + recycle gaps (P99 spikes) | `recycle_workers()` replaces threads; no hot standby | 0.3 |
| **P4-B** | **Zero-copy request buffer (embed)** | JSON/MessagePack serialize cost on large bodies | IPC JSON; embed stdio JSON; gateway→pool copies | 0.3–0.4 (with libphp FFI) |
| **P4-C** | **Image JIT + opcode pinning** | PHP 8.5 JIT warm-up on cold workers | Not in image build yet | 0.3 (`nusa-runtime` build) |
| **P4-D** | **Transparent async I/O offload** | PHP threads blocked on DB/HTTP | Laravel uses normal PDO; gateway async only at edge | 0.4+ (spike first) |
| **P4-E** | **Static-first routing** | PHP serving assets and cacheable HTML | Tier-S1 `StaticFileHandler` before PHP (GET/HEAD) | 0.2 ✓ extend cache/MIME |
| **P4-F** | **Hybrid static + partial dynamic (ESI)** | Full Blade render for mostly-static pages | Not implemented | Post-GA / optional |

#### P4-A — Shadow worker pool

- **Idea:** Maintain `N` active workers plus `M` standby workers that already ran `bootstrap/app.php` (or `nusa_embed_bootstrap`). On recycle or traffic spike, swap active ↔ standby; cool down and reset the retired worker asynchronously before returning it to standby.
- **Why:** Cold Laravel boot dominates P99 when `octane_max_requests` or memory limits trigger recycle — same class of problem as V8 JIT warm-up, but at framework level.
- **Design notes:** Reuse `StateResetOrchestrator` events; cap `M` via config (e.g. `octane_standby_workers`); fail-closed if standby never reaches `is_ready()`.
- **Gate:** P99 recycle spike ≤ baseline IPC without recycle under `just podman-bench-normal-smoke` on `/nusa-ping`.

#### P4-B — Zero-copy embed buffer

- **Idea:** `mmap` (or shared arena) writable by gateway/thread Rust and read by ZTS PHP thread via FFI — request line, headers, body, response written in-place; bridge reads pointers instead of `json_decode` / framed JSON.
- **Why:** Removes dominant copy/parse cost for embed path; IPC may keep framed JSON or optional MessagePack as **dev fallback only** (not end state per plan).
- **Design notes:** Versioned buffer header (magic, length, flags); strict bounds checks; Landlock on mapping; one buffer per in-flight request per worker thread.
- **Depends on:** `embed-php` feature, `nusa-engine-ffi` thread init + `nusa_embed_handle_request` from memory.
- **Gate:** Byte-for-byte Laravel response parity vs stdio embed on fixture routes; bench row S2-embed P50 improves vs S2-ipc on 64 KiB body fixture.

#### P4-C — JIT and opcode profile per image

- **Idea:** During `nusa-runtime` image build, run a short route smoke (fixture or app-specific) and bake:
  - `opcache.jit_*` tuned for PHP 8.5
  - Pre-warmed opcode cache volume or embedded cache layer
- **Why:** JIT hot-path discovery otherwise needs thousands of production requests.
- **Design notes:** Document in [embed-runtime-image.md](../embed-runtime-image.md); optional `NUSA_JIT_PROFILE=1`; never required for dev host.
- **Gate:** First-request P50 on `/nusa-ping` within X% of steady-state (TBD in [normal-mode-report.md](../../benchmarks/normal-mode-report.md)).

#### P4-D — Transparent async I/O (killer feature, high risk)

- **Idea:** Laravel code stays synchronous (`User::find(1)`); Nusa provides a **PDO / HTTP client proxy** that submits work to Rust async pools and parks the PHP worker until completion (Swoole/OpenSwoole-class, orchestrated from Rust).
- **Why:** Most Laravel apps are I/O-bound; freeing PHP threads raises throughput without faster opcodes.
- **Risks:** Semantic gaps (transactions, lazy relations, debugging), extension compatibility, test matrix explosion.
- **Path:** Spike in isolated crate → opt-in `NUSA_ASYNC_IO=1` → never default until KPI proven on Laravel minimal + one real app.
- **Non-goal for v0.2:** Full Eloquent driver rewrite.

#### P4-E — Static-first (in progress)

- **Done (v0.2):** Gateway serves `static_root` / `{code_dir}/public` for GET/HEAD before `LaravelHttpRuntime`.
- **Done:** `StaticFileHandler::is_static()` gate avoids disk I/O on dynamic paths (e.g. `/api/users`).
- **Done:** `static_cache_*_max_age_secs` in config; `X-Nusa-Tier: S1` + `nusa_static_served_total` metric; HEAD returns headers + `Content-Length` without body.
- **Done (Tier-S2):** Plugin `pre_exec` may `set_short_circuit` → `nusa_tier_s2_short_circuit_total`.
- **Done:** build-time precompressed `.br`/`.gz` siblings when `Accept-Encoding` matches (br preferred).
- **Done:** `static_cache_max_entries` + `static_cache_ttl_secs` for moka LRU.

#### P4-F — ESI / partial Blade in Rust (deferred)

- Split layout (header/footer static in Rust) + dynamic “hole” from PHP. High complexity and cache invalidation risk.
- **Defer** until P4-A/B/E and KPI K1–K3 are met; prefer plugins `pre_exec` short-circuit (Tier-S2) over Blade fragmentation.

### Recommended implementation order

| Priority | Item | Rationale | Status |
|----------|------|-----------|--------|
| 1 | **P4-E** (extend static) | Low risk; already in gateway | Done |
| 2 | **P4-A** (shadow pool) | Directly improves Octane recycle P99 | Done |
| 3 | **P4-C** (JIT image) | Cheap win when `nusa-runtime` embed lands | Done (OPcache) |
| — | **Stability Sprint 1–3** | Hot path: idle_queue, body copy, Mutex removal, frame default | Done (May 2026) |
| 4 | **P4-B full mmap** (FFI zero-copy) | Largest embed perf win; depends on libphp in-process | Pending |
| 5 | **P4-D** (async I/O production) | Differentiator vs FrankenPHP; spike done, production pending | Spike done, production pending |
| — | **P4-F** | Post-GA optional | Deferred |

**Explicitly out of scope:** Rewriting Zend/libphp in Rust (see external experiments e.g. php.rs — watch, do not duplicate inside Nusa).

### Phase 4 gates (pre-GA)

- [x] P4-A shadow pool (`octane_standby_workers`, warm swap on recycle); no regression in `just podman-ci`
- [x] P4-B NEB1 frame default (`NUSA_EMBED_TRANSPORT` defaults to frame; JSON fallback via `=json`)
- [ ] P4-B full mmap zero-copy (FFI/mem-mapped; gate: byte-for-byte Laravel parity vs stdio embed)
- [x] P4-C OPcache/JIT ini + `warm-php-opcache.sh` in CI image build
- [x] P4-D spike doc ([async-io-offload.md](../spike/async-io-offload.md) + `nusa-core::async_io`); [x] Rust read-only SQL stub (`NUSA_ASYNC_IO=stub`, allowlist); [x] Laravel blocking `/nusa-db-ping`; [x] Embed NEB1 op 6/7 (`/nusa-async-spike`, `/nusa-async-sql`); [x] Laravel sqlite proxy spike (`NusaAsyncSqliteConnection`, `/nusa-db-async-proxy`); [x] pooled SQLite connection (no open-per-query); [ ] production PDO (bindings, writes, pool metrics, true async park)
- [x] KPI K1–K2: external baselines documented in [normal-mode-report.md](../../benchmarks/normal-mode-report.md); internal FPM/FrankenPHP benchmark not run (policy: use public benchmarks)

### Stability + efficiency fixes (Sprints 1–3, 2026-05-23)

| Fix | File | Impact |
|-----|------|--------|
| Removed `idle_queue.contains()` O(n) scan on `return_worker` | `nusa-octane-worker/src/pool.rs` | O(1) worker return instead of O(n) per request |
| Removed `idle_queue.retain()` O(n²) drain path | `nusa-octane-worker/src/pool.rs` | WorkerState alone gates draining; no queue scan |
| `SpikeSqliteAsyncIoBridge` uses pooled connection | `nusa-core/src/async_io.rs` | One SQLite conn shared; no open/prepare per query |
| Deferred `ctx.clone()` — only in Octane Pool path | `nusa-gateway/src/lib.rs` | Engine/static requests skip clone cost |
| Zero body copy `build_http_response` — `Body::from(Bytes)` direct | `nusa-gateway/src/lib.rs` | 2 body copies → 0 |
| Eliminated `request_id.clone()` — move semantics | `nusa-gateway/src/lib.rs` | Unnecessary String clone removed |
| Removed `Mutex<StateResetOrchestrator>` — `emit_event` is `&self` | Gateway, CLI, 13+ test files | 2 lock acquisitions per request removed |
| NEB1 frame transport = default for embed | `nusa-engine-embed/src/stdio_worker.rs` | Binary frame instead of JSON serialize |
| Eliminated Vec third allocation in `read_bytes` — splice in-place | `nusa-engine-embed/src/stdio_worker.rs` | 1 Vec less per frame read |
| External KPI baselines + benchmark policy | `docs/benchmarks/normal-mode-report.md` | 8 refs from 4 sources; no internal FPM/FrankenPHP runs |

## References

- [`crates/nusa-engine-embed/`](../../../crates/nusa-engine-embed/)
- [`php-driver/src/Embed/`](../../../php-driver/src/Embed/)
- [`docs/public/laravel/octane-mode.md`](../../public/laravel/octane-mode.md)
- [`docs/contributor/embed-runtime-image.md`](../embed-runtime-image.md)
