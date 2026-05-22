# Security development

How sandboxing and security tests work in Nusa.

## Enforcement stack

| Mechanism | Crate | Linux behavior |
|-----------|-------|----------------|
| Landlock | `nusa-security/src/landlock.rs` | Restrict FS to `code_dir`, `tmp_dir` |
| Seccomp | `nusa-security/src/seccomp.rs` | Syscall filter for PHP workers |

Applied in CLI **before** `axum::serve` ([`main.rs`](../../crates/nusa-cli/src/main.rs)).

## Tests

| Suite | Path |
|-------|------|
| Core security | `crates/nusa-security/tests/security_test.rs` |
| Extended | `crates/nusa-security/tests/security_extended_test.rs` |
| Enforcement | `crates/nusa-security/tests/security_enforcement_test.rs` |

On `target_os = "linux"`, `apply_landlock` / `apply_seccomp` must succeed in Alpine CI or tests **fail** with actionable errors — no `eprintln` skip.

## Documentation

- [Threat model](../security/threat-model.md)
- [Compliance](../security/compliance.md)
- [SECURITY.md](../../SECURITY.md) — disclosure policy

## Development rules

- No weakening seccomp/Landlock rules to green tests without security review
- New surface area (gateway routes, IPC) needs threat model update
- `unsafe` only in `nusa-engine-ffi` with `// SAFETY:` comments

## FFI / WASM notes

- FFI links libphp — supply chain and ZTS build are operator concerns
- WASM engine in CLI is a **stub** — do not document as sandboxed production PHP until implemented
