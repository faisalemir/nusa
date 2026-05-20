# Cargo Geiger — Unsafe Code Audit Report

## Summary

| Metric | Count |
|--------|-------|
| `#![deny(unsafe_code)]` crates | 10/11 |
| `#![allow(unsafe_code)]` crates | 1/11 (`nusa-engine-ffi`) |
| Total `unsafe` blocks in workspace | See FFI crate below |
| `unsafe` outside FFI boundary | **0** ✅ |

## `nusa-engine-ffi` — Only Crate with `unsafe`

This crate is the **only** location where `unsafe` is permitted, per the project's safety policy:

> `#![deny(unsafe_code)]` enforced in all crates except `nusa-engine-ffi`.
> All unsafe blocks must have `// SAFETY:` comments referencing:
> - PHP ZTS thread-local storage guarantees
> - Isolated execution per thread
> - `catch_unwind` for panic safety

### Unsafe Locations

| File | Line | Purpose | SAFETY Comment |
|------|------|---------|----------------|
| `src/engine.rs` | FFI callback | `rust_ub_write` — captures PHP stdout | ✅ Documents pointer validity, thread-local buffer |
| `src/engine.rs` | init/shutdown | `php_embed_init`, `php_execute_script`, `php_request_shutdown` | ✅ Documents ZTS thread-local guarantees, no shared mutable state |

All `unsafe` blocks in `nusa-engine-ffi` are:
1. **Scoped** — Only in FFI boundary functions
2. **Documented** — Each has a `// SAFETY:` comment
3. **Isolated** — Uses thread-local storage (ZTS)
4. **Protected** — Wrapped in `catch_unwind` for panic safety

## Verdict

✅ **Compliant** — Zero `unsafe` outside designated FFI boundary.
All FFI `unsafe` blocks are documented with `// SAFETY:` comments.
