# Development workflow

## Prerequisites

- Rust stable (`rust-toolchain.toml`)
- `just` (command runner)
- Podman or Docker for Alpine CI parity

## Daily commands

| Task | Command |
|------|---------|
| Format | `just fmt` |
| Lint (deny warnings) | `just lint` |
| Fast tests (no rebuild) | `just test-fast` |
| Single crate | `just test-crate core` |
| New/extended tests only | `just test-new` |
| API docs | `just docs` |
| One benchmark | `just bench-fast ipc_latency_bench` |

## Before push / PR

```bash
just fmt-check
just podman-ci
```

`just ci` on Windows/macOS is **not** a substitute for `just podman-ci`.

## Build binary

```bash
cargo build -p nusa-cli --release
```

## Run locally

```bash
cp config.toml.example nusa.toml
cargo run -p nusa-cli -- --config nusa.toml
```

## Musl release build

```bash
just build-musl
```

## Code standards

- `#![deny(unsafe_code)]` except `nusa-engine-ffi`
- No `unwrap()` in libraries
- Public items need `///` docs
- Prefix new symbols with `nusa_` / `Nusa` / `NUSA_` per project rules

## Skills

Load from `.cursor/skills/` — especially `rust-test`, `rust-design-pattern`, `m10-performance` for gateway/IPC changes.

## AI-assisted work

Read [`AGENTS.md`](../../AGENTS.md) first; edit only hotspot files from [`docs/ai/hotspots.md`](../ai/hotspots.md).
