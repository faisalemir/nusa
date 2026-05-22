# Getting started with Nusa

Welcome. You are about to run a runtime that treats Laravel as a **first-class citizen** in a **systems-language control plane**—not as a CGI script behind Apache.

Nusa PHP Runtime (**v0.1.0**, pre-GA) combines:

- An **Axum-powered HTTP gateway** with middleware, circuit breaking, and backpressure
- **Pluggable PHP engines** (child processes today; FFI and WASM paths for specialized deployments)
- **Kernel sandboxing** (Landlock + seccomp) applied before `listen()`
- A roadmap-complete **Octane worker pool** over a binary IPC protocol (HTTP routing into the pool is finishing in P1)

Read [Production status](production-status.md) before production cutover. For experiments, labs, and staging shaped like Alpine production, you are in the right place.

---

## The mental model in sixty seconds

```
  Browser / API client
           │
           ▼
  ┌─────────────────────────────────────┐
  │  Nusa Gateway (Rust)                │
  │  • TLS / HTTP / future QUIC         │
  │  • Tenant + trace + rate limits     │
  │  • Circuit breakers + backpressure  │
  │  • /health /ready /metrics          │
  └──────────────┬──────────────────────┘
                 │
       ┌─────────┴─────────┐
       ▼                   ▼
  Normal mode          Octane mode
  PhpEngine per        Worker pool + IPC
  request path         (HTTP dispatch → P1)
       │                   │
       ▼                   ▼
  PHP (child / ffi)    Laravel Octane worker
```

**Normal mode** (`octane_workers = 0`): each request flows through `PhpEngine::execute`—think “FPM semantics with Rust supervision.”

**Octane mode** (`octane_workers > 0`): the CLI already spins up persistent workers and state-reset orchestration; the gateway will forward HTTP to that pool as P1 lands. Today, HTTP still uses the engine path—plan accordingly.

---

## Prerequisites

| Component | Why you need it |
|-----------|-----------------|
| **Rust toolchain** | Build the `nusa` binary from source; see `rust-toolchain.toml` |
| **PHP 8.x** | Child engine and Octane workers execute real PHP |
| **Linux (recommended)** | Landlock and seccomp match production enforcement; macOS/Windows are fine for dev builds but not authoritative for sandbox proofs |
| **Podman or Docker (recommended)** | Reproduce Alpine musl CI with `just podman-ci` before you merge infrastructure changes |

---

## Build the runtime

From the repository root:

```bash
cargo build -p nusa-cli --release
```

The binary is named **`nusa`** (crate `nusa-cli`). One artifact replaces separate “app server + process manager + fragile shell glue.”

---

## Configure your Laravel layout

Configuration is **`nusa.toml`** (or any path you pass with `--config`). It merges TOML with **`NUSA_`-prefixed environment variables**—ideal for Kubernetes ConfigMaps and Secrets without templating PHP files.

```bash
cp config.toml.example nusa.toml
```

Edit these paths with care—they double as **security policy inputs**:

| Setting | Role |
|---------|------|
| `code_dir` | Laravel application root; Landlock “code” rules |
| `vfs_root` | Document root semantics for PHP script resolution |
| `tmp_dir` | Writable enclave for uploads, caches, sessions |

Full reference: [Configuration](configuration.md).

---

## Start the server

Development (compile + run):

```bash
cargo run -p nusa-cli -- --config nusa.toml
```

Production-style binary:

```bash
./target/release/nusa --config nusa.toml
```

When the process is up, you have a single observability surface:

```bash
curl -s http://localhost:8080/health    # liveness
curl -s http://localhost:8080/ready     # readiness (see production status for Octane caveats)
curl -s http://localhost:8080/metrics   # Prometheus exposition
```

Hit any application route (`/`, `/api/...`) to exercise the PHP engine path.

---

## Developer experience: `nusa dev`

Local Laravel work should feel fast. The **`dev`** subcommand watches the directories that actually matter—`app/`, `config/`, `routes/`, `resources/views/`, `.env`—with debounced reload and optional pretty logs:

```bash
cargo run -p nusa-cli -- dev --pretty
```

You keep Laravel’s productivity; Nusa owns process supervision and config hot-reload (`hot_reload = true` swaps config via `ArcSwap` without dropping the listener).

---

## Octane mode (preview—read before enabling)

Set `octane_workers` to the number of persistent Laravel workers you want (for example `4`). The runtime will:

1. Spawn workers via `php-driver/bin/octane-rust-worker`
2. Perform IPC handshake and heartbeat over `nusa-ipc`
3. Track memory and request budgets for **worker recycling**

**Important for v0.1.0:** HTTP requests still route through `engine.execute` until gateway dispatch (P1) ships. Use Octane configuration today to validate worker bootstrap and IPC in staging—not as a silent drop-in for RoadRunner throughput yet.

Install the PHP driver per [ecosystem/package-guidelines.md](ecosystem/package-guidelines.md).

---

## Where to go next

| Goal | Document |
|------|----------|
| Understand maturity and gaps | [Production status](production-status.md) |
| Migrate from FPM or RoadRunner | [Migration](migration.md) |
| Operate under incidents and SLOs | [Operations runbook](operations/runbook.md) |
| Tune engines and limits | [Configuration](configuration.md) |

Contributors and Rust hackers: [CONTRIBUTING.md](../../CONTRIBUTING.md) → [contributor docs](../contributor/).

---

## A note on ambition vs honesty

Nusa is built because **PHP deserves a control plane as serious as Go or Rust services enjoy**—without asking Laravel developers to abandon PHP. This guide gets you running quickly; [Production status](production-status.md) tells you when “running” becomes “betting the business.” Both matter, and we will not blur them.
