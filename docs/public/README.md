# Nusa — public documentation

**Nusa PHP Runtime** is not another PHP-FPM wrapper. It is a deliberate re-architecture of how Laravel meets the network: a **memory-safe Rust control plane** that owns TLS, routing, backpressure, tenancy, observability, and kernel-level sandboxing—while PHP remains the language of your application logic.

Where traditional stacks bolt a web server onto PHP and hope for the best, Nusa inverts the relationship. Rust sits at the edge because that is where concurrency, security policy, and operational truth belong. PHP executes inside a governed boundary you configure once and enforce everywhere.

> *Rust orchestrates. Laravel performs. The platform protects both.*

This documentation is for **operators, platform engineers, and Laravel teams** who will run Nusa in production—not for Rust contributors (see [contributor docs](../contributor/)).

---

## Why teams choose this architecture

### One runtime, two performance personalities

Nusa is designed around a single gateway with **two execution philosophies**, switchable by configuration:

| Mode | Philosophy | Best for |
|------|------------|----------|
| **Normal** | FPM-compatible isolation—bootstrap per request through a governed `PhpEngine` | Migrations from nginx/php-fpm, predictable cold paths, maximum isolation |
| **Octane** | Long-lived Laravel workers over a framed **IPC contract**—bootstrap once, serve many | Throughput, warm caches, Octane-style latency (full HTTP dispatch completing in P1) |

You do not maintain two products. You tune `nusa.toml` and let the runtime adapt.

### Security is not a PHP ini setting

Before the gateway accepts traffic, Nusa applies **Landlock** filesystem rules and **seccomp-BPF** syscall filtering from Rust. That means your Laravel app cannot silently widen the attack surface through a misconfigured `open_basedir` or a forgotten upload path. Policy is enforced at the kernel boundary, with tests that **fail closed** on Linux CI—not skipped with a friendly log line.

### Built for multi-tenant SaaS from day one

The gateway understands **tenant identity** from headers and hostnames, applies **per-tenant rate limits** and **per-tenant circuit breakers**, and routes observability context through **W3C TraceContext**. You get platform primitives that usually require a service mesh sidecar—embedded in the same binary that serves HTTP.

### Observable by construction

Prometheus metrics, structured JSON logs, health and readiness probes, WebSocket and SSE endpoints, and async task offload APIs are first-class routes—not afterthoughts. Nusa treats operability as a feature equal to request handling.

### Cloud-native without framework lock-in

Hot-reload configuration via `ArcSwap`, Alpine **musl** as the production target, container-first CI (`just podman-ci`), and optional ACME/TLS/QUIC modules in the gateway crate map to how modern platforms actually ship software: immutable images, declarative config, measurable SLOs.

---

## Documentation map

| Document | What you will learn |
|----------|---------------------|
| [Getting started](getting-started.md) | Build, configure, and feel the runtime in minutes |
| [Configuration](configuration.md) | Every `nusa.toml` key—and *why* it exists |
| [Production status](production-status.md) | Transparent maturity: what is production-grade today vs next |
| [Migration](migration.md) | Leaving FPM, RoadRunner, or FrankenPHP without surprises |
| [Operations runbook](operations/runbook.md) | How SRE teams run Nusa under load and incident stress |
| [PHP ecosystem](ecosystem/package-guidelines.md) | How Composer and the Octane worker bridge into Rust |

---

## Security and compliance

- [Threat model](../security/threat-model.md) — how we think about IPC, VFS escape, and supply chain
- [Compliance](../security/compliance.md) — evidence-oriented mapping for regulated environments
- [SECURITY.md](../../SECURITY.md) — responsible disclosure

---

## Version honesty (read this once)

The workspace ships **v0.1.0 (pre-GA)**. The **ideas and most subsystems are real**—gateway, engines, sandbox, IPC, worker pool, extensive tests—but we document gaps openly (for example, Octane HTTP dispatch through the pool is the active P1 milestone).

We would rather earn your trust with precision than lose it with marketing adjectives. [Production status](production-status.md) is the single source of truth for “can I bet my company on this today?”

---

## Next step

Start with **[Getting started](getting-started.md)**—then read **[Production status](production-status.md)** before you point staging traffic at Nusa.
