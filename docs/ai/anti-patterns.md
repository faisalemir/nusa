# AI anti-patterns

Forbidden patterns for agents working on Nusa. Violations fail review and CI policy.

## Documentation

| Anti-pattern | Why |
|--------------|-----|
| Copying old README “Phase 1–4 complete” / “58 tests” | Misleading; use `just podman-test-full` |
| Documenting `just run` / `just build` without checking `justfile` | Those recipes may not exist |
| Claiming `/admin/recycle-all` | Route not in gateway |
| Claiming WASM engine runs real PHP in CLI | `WasmEngine::stub()` in `main.rs` |
| Claiming Octane HTTP works because pool initializes | Handler must dispatch to pool |
| Using `./bin/ask` or external doc validators | Use **docs-expert** + `justfile` |

## Tests

| Anti-pattern | Example |
|--------------|---------|
| Silent skip on Linux enforcement | `if ok { assert } else { eprintln!(); return }` |
| Stub success as production | `assert!(stub.handle_request().is_ok())` when contract is `Err` |
| `#[ignore]` without `// REASON:` and issue | — |
| `--retries > 0` to hide flakes | Project uses `--retries 0` |
| Host-only green for merge | Must run `just podman-ci` for test changes |

## Code quality

| Anti-pattern | Fix |
|--------------|-----|
| `#[allow(clippy::all)]` | Fix root cause |
| `let _ =` on `#[must_use]` Result | Handle or document |
| `unwrap()` in library crates | `?` or `expect("invariant: …")` |
| `unsafe` without `// SAFETY:` | Required in FFI |
| `panic!` for recoverable errors | Return `Result` |

## Performance

| Anti-pattern | Fix |
|--------------|-----|
| Optimize without measurement | `just bench-fast <name>` |
| Claim faster without release bench | Criterion in `benches/` |

## Git

| Anti-pattern | Fix |
|--------------|-----|
| Commit without user request | Ask first |
| `git push --force` to main | Warn user |
