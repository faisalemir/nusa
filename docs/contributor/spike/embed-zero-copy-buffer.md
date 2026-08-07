# Spike: P4-B embed binary frame (`NEB1`)

**Status:** Spike implemented (stdio opt-in)  
**RFC:** [nusa-native-laravel-runtime.md](../rfc/nusa-native-laravel-runtime.md) § P4-B

## Goal

Remove JSON encode/decode on the Rust ↔ PHP embed hot path before libphp in-process FFI ships. Headers stay JSON **inside** the binary blob for v1; bodies are raw bytes.

## Wire format v1

Outer envelope (unchanged from JSON stdio):

```text
u32 LE inner_len
inner_payload
```

Inner payload:

```text
magic[4]   = "NEB1"
version[1] = 1
op[1]
pad[2]     = 0
... op-specific fields ...
```

| op | Name | Body |
|----|------|------|
| 1 | Bootstrap | `u32 code_dir_len` + UTF-8 path |
| 2 | Ack | empty |
| 3 | Request | `u16 method_len`, `u32 uri_len`, `u32 hdr_json_len`, `u32 body_len`, payload |
| 4 | Response | `u16 status`, `u32 hdr_json_len`, `u32 body_len`, payload |
| 5 | Error | `u32 msg_len` + UTF-8 |
| 6 | AsyncQuery (P4-D) | `u32 sql_len` + UTF-8 |
| 7 | AsyncResult (P4-D) | `u8` ok + kind (`0`=i64, `1`=JSON rows) + payload; or `u8` err + msg |

## Code map

| Component | Path |
|-----------|------|
| Rust encode/decode | [`crates/nusa-engine-embed/src/frame.rs`](../../crates/nusa-engine-embed/src/frame.rs) |
| Stdio worker switch | [`crates/nusa-engine-embed/src/stdio_worker.rs`](../../crates/nusa-engine-embed/src/stdio_worker.rs) |
| PHP codec | [`php-driver/src/Embed/FrameCodec.php`](../../php-driver/src/Embed/FrameCodec.php) |
| Daemon | [`php-driver/embed/nusa_embed_daemon.php`](../../php-driver/embed/nusa_embed_daemon.php) |

## Enable

```bash
export NUSA_EMBED_TRANSPORT=frame
export NUSA_OCTANE_BACKEND=embed
nusa --config nusa.toml
```

Default remains JSON for compatibility.

## v2 (true zero-copy)

- `mmap` region per worker thread (Rust gateway writer, PHP FFI reader).
- Same field layout; pointers instead of stdin copy.
- Landlock on mapping; one in-flight request per thread.

## Bench plan

`tests/load/run-alpine-bench-smoke.sh` records **S2-embed JSON** and **S2-embed frame** rows in `docs/benchmarks/artifacts/normal-smoke-*.md`. Compare P50/RPS on `/nusa-ping`; optional 64 KiB POST body follow-up in [`normal-mode-report.md`](../../benchmarks/normal-mode-report.md).
