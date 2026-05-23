---
name: rust-test
description: "Generate exhaustive, deep-dive test cases for Rust. Default skill for all testing. No token limits. Covers every edge case, security vector, concurrency race, memory pattern, and real-world failure mode. Triggers: test cases, unit test, integration test, test coverage, test pattern, test strategy, #[test], #[tokio::test], cargo nextest, cargo test, prop_test, fuzz testing, security test, boundary test, error test, regression test, stress testing, leak detection, comprehensive audit, 测试用例, 单元测试, 集成测试, 测试覆盖, 全面测试, 深度测试, 安全审计测试"
source: https://github.com/actionbook/rust-skills
user-invocable: false
---

# Rust Test Cases

> **Default testing skill. No token limits. Generate EVERY test. Coverage over efficiency.**
>
> Based on the [rust-skills](https://github.com/actionbook/rust-skills) framework by @actionbook — adapted with exhaustive test patterns, security matrices, and cross-skill tracing for comprehensive Rust testing.

## Routing / Quality Gate (READ FIRST)

Before generating any test cases, classify the target to determine scope. Then generate exhaustively.

---

## Production Test Integrity (Alpine Linux) — MANDATORY

Read `nusa-standards.mdc` → **Production Test Integrity**. Every test plan and generated test must comply.

### Source of truth

- **Production target:** Alpine Linux musl (`rust:1.95-alpine`, `musl-dev`, static/musl binary path)
- **Authoritative gate:** `just podman-ci` or `just podman-test` — tests run inside `nusa-test-runner` image, native ext4, no Windows volume-mount distortion
- **Host `just test`:** development only; never treat as merge-ready without Alpine run

### DO — honest tests

| Practice | Requirement |
|----------|-------------|
| Fail closed on Linux | If `apply_landlock` / `apply_seccomp` / real IPC fails on `target_os = "linux"`, test **fails** with message pointing to Alpine CI — not `eprintln` + pass |
| Stub contract | Prefix `// STUB_CONTRACT:` — stub proves API shape or expected error; integration proves behavior in `podman-test` |
| Enforcement assertions | After apply succeeds, assert **denied** action (EACCES, timeout, `is_err`) — not only that apply returned `Ok` |
| Real timeouts | Use production-scale timeout in integration tests; document if shorter only in unit isolation |
| PR honesty | State: Alpine run yes/no, stub vs real path, any new `#[ignore]` |

### DON'T — gaming (reject in review)

| Anti-pattern | Why forbidden |
|--------------|---------------|
| `if result.is_ok() { assert!... } else { eprintln!("skip") }` | Passes when enforcement unavailable — hides production failure |
| `assert!(true)` / empty test body | Fake coverage |
| `#[ignore]` without `// REASON:` + issue | Hides failing tests |
| Widen `timeout` / add retries until flake disappears | Masks race or bug |
| nextest `-E` filter dropping failing binaries | Cherry-pick green |
| Stub `handle_request` → `is_ok()` when production requires Unix socket | Green-wash |
| `#[cfg(windows)]` only E2E for Linux-only feature | Missing Alpine coverage |
| Seccomp/Landlock Linux test with empty `#[cfg(linux)]` no-op body | False sense of coverage |

### Alpine-specific test matrix (when applicable)

| Feature | Must run in `podman-test` |
|---------|----------------------------|
| Landlock FS rules | `security_enforcement_test` — write outside code_dir → `PermissionDenied` |
| Seccomp filter build/apply | Linux tests — no privilege skip on Alpine CI image |
| Unix domain IPC / worker socket | Not satisfied by Windows stub worker alone |
| musl linking / OpenSSL | Alpine image deps match production Dockerfile |

### Stub worker / engine tests (Octane, child engine)

```rust
// STUB_CONTRACT: non-Unix returns stub; handle_request must Err with "stub mode".
let result = worker.handle_request(...).await;
assert!(result.is_err(), "stub must not fake production success");
```

Generate **parallel** Linux integration tests (or `podman-test` binary) for paths stubs cannot cover.

---

## Nusa Test Sector Registry (READ when adding or auditing tests)

Map work to a **sector** first, then to the detailed matrix section below (PHP-Rust Integration, Advanced Runtime, Phase 1–4). Do not add tests without naming the sector and gate.

### Authoritative gates (Alpine production)

| Gate | Command | Sectors covered |
|------|---------|-----------------|
| P0 merge | `just podman-ci` | Workspace minus `nusa-cli` (see `justfile` excludes) + Laravel live subset |
| P0 full Alpine | `just podman-test-full` | All workspace crates with live mount |
| P2 pre-GA | `just podman-ci-e2e` | P0 + `nusa-e2e-tests` leak suite + `ipc_latency_bench` smoke |
| P2 Laravel only | `just podman-test-laravel` / `just podman-test-laravel-leak` | Sector 15 |
| Per-crate | `just podman-test-pkg CRATE` | Single sector validation |
| Host dev | `just test-fast` / `just test-crate` | **Not merge evidence** |

### Sector map (crate → files → matrix → status)

| ID | Sector | Primary paths | Test binaries (representative) | Skill matrix anchor | Gate | Coverage |
|----|--------|---------------|--------------------------------|---------------------|------|----------|
| S01 | Gateway HTTP + middleware | `nusa-gateway/src/lib.rs`, `middleware.rs` | `gateway_test`, `middleware_*`, `gateway_integration_test` | Request flow §1020 | `podman-test` | **Strong** |
| S02 | Gateway Octane dispatch | `handler`, `octane_dispatch` | `octane_dispatch_test`, `test_fake_ipc` | Request flow + Octane §1052 | `podman-test-laravel` | **Medium** — fake IPC dispatch covered; Laravel live in P2 |
| S03 | WebSocket / SSE | `websocket.rs`, `sse.rs` | `websocket_*`, `sse_*` | Advanced runtime | `podman-test` | **Medium** — e2e present; load/failure paths |
| S04 | TLS / ACME / QUIC | `acme.rs`, `quic.rs` | `tls_*`, `tls_acme_test`, `quic_e2e_test` | Deployment + protocol §931 | `podman-test` | **Medium** — experimental (P5); no fake "GA green" |
| S05 | Static files + CDN path | `static_files.rs` | `static_files_*` | Request flow | `podman-test` | **Strong** |
| S06 | Circuit breaker + tenant | `circuit_breaker.rs`, `tenant_*` | `circuit_breaker_test`, `tenant_circuit_breaker_test` | Security hardening §1278 | `podman-test` | **Strong** |
| S07 | Blue-green + lifecycle | `bluegreen.rs`, health | `bluegreen_test`, `lifecycle_test` | Lifecycle §949 | `podman-test` | **Medium** — partial impl; tests must `STUB_CONTRACT` |
| S08 | Octane worker pool | `nusa-octane-worker/pool.rs` | `pool_*`, `worker_domain_test`, `integration_test` | Octane §1052 | `podman-test` + **S15** | **Strong** unit; IPC body matrix still growing |
| S09 | IPC transport + protocol | `nusa-ipc/` | full `*_exhaustive_*` suite (9 files) | Protocol §931, IPC boundary §915 | `podman-test` + bench smoke | **Strong** |
| S10 | Core domain (VFS, guards, tenant) | `nusa-core/` | `vfs_*`, `guards_*`, `tenant_*`, `*_exhaustive_*` | Phase 1–4 + state §1106 | `podman-test` | **Strong** |
| S11 | Config + hot reload | `nusa-config/` | `config_*`, `decision_exhaustive_*`, `platform_test` | Hot reload §1206 | `podman-test` + `hot_reload_e2e` | **Strong** |
| S12 | Security sandbox | `nusa-security/` | `security_enforcement_*`, `security_exhaustive_*`, `resource_exhaustive_*` | Security §1278 | `podman-test` | **Medium** — add `stress_decision`, `decision_exhaustive` |
| S13 | Telemetry + metrics | `nusa-telemetry/` | `export_*`, `metrics_*`, `*_exhaustive_*` | Observability §1349 | `podman-test` + `observability_e2e` | **Medium** — OTLP/export failure paths |
| S14 | Plugin hooks | `nusa-plugin-api/` | `plugin_*`, `*_exhaustive_*`, `stress_decision` | Plugin §1230 | `podman-test` + `plugin_e2e` | **Medium** — deregister API still absent by design |
| S15 | Laravel live + leak | `nusa-e2e-tests/`, fixture | `laravel_live`, `octane_leak_suite`, `trace_propagation` | Laravel §1254 | **`podman-ci-e2e`** only | **P2** — real PHP; never stub-pass on Unix without fixture |
| S16 | Engines (child / FFI / WASM) | `nusa-engine-*` | `*_domain_*`, `*_security_*`, stubs | Engine §1024 | FFI: **manual** in image; WASM stub in CLI | **P1 gap** — no `resource_*` / `stress_decision`; WASM must not claim PHP parity |
| S17 | CLI (dev, test runner) | `nusa-cli/` | `cli_*`, `dev_watcher_*`, `test_runner_*` | domain-cli | **Excluded** default podman | **P1 gap** — run `podman-test-pkg cli` when touched |
| S18 | Workspace cross-crate | `tests/integration/` | `full_lifecycle`, `multi_tenant`, `engine_integration`, `plugin_e2e`, `observability_e2e` | Cross-boundary §915 | `podman-test-full` | **Medium** — tie to S02/S15 for Octane path |
| S19 | Embed engine (NEB1 frame, stdio pool, async I/O) | `nusa-engine-embed/` | `frame.rs`, `stdio_worker.rs`, `pool.rs`, `laravel_runtime_impl.rs`, `error.rs`, `paths.rs` | Protocol §931, Embed §P4-B/D | `podman-test-laravel` + bench smoke | **P0 gap** — new crate; frame fuzzing, SQL injection allowlist bypass, pool lifecycle, zero-copy security boundary not yet covered |
| S20 | PHP-Rust async I/O bridge | `nusa-core::async_io`, `php-driver/src/Embed/AsyncIo.php`, `NusaAsyncSqliteConnection.php` | `async_io.rs` (Rust), PHP driver tests | P4-D spike, async offload | `podman-test-laravel` | **P0 gap** — read-only SQL allowlist bypass, write injection, PDO proxy injection, env variable tampering not yet covered |

### Required file patterns per sector (when adding depth)

For each sector marked **P0/P1 gap**, add or extend:

| Pattern | Purpose |
|---------|---------|
| `security_exhaustive_test.rs` | Injection, auth bypass, malicious input |
| `resource_exhaustive_test.rs` | Limits, timeout cleanup, orphan resources |
| `concurrency_extended_test.rs` | Races under parallel load |
| `stress_decision_test.rs` | Decision tables under stress |
| `decision_exhaustive_test.rs` | Guard/config combinatorics |
| `*_e2e_test.rs` | Real stack (Alpine); `STUB_CONTRACT` if partial |
| `*_domain_test.rs` | Business/state machine per `rust-design-pattern` |

### P0 scenarios to implement next (evidence-backed)

1. **S02** — Gateway dispatch + `octane_pool::init_octane_pool` fail-closed (`octane_init_test`); Laravel gateway: `laravel_gateway_octane_dispatch_integration`.
2. **S15** — POST/query/counter routes in fixture; remaining: session/middleware matrix, `artisan-octane-contract.sh`, 10k leak (`just podman-ci-e2e`).
3. **S12** — `security_exhaustive` + `resource_exhaustive`; optional: `stress_decision` / `decision_exhaustive` for seccomp combinatorics.
4. **S19** — NEB1 frame fuzzer, `is_readonly_sql` bypass matrix, pool worker exhaustion, bootstrap path injection, stdio transport MITM.
5. **S20** — SQL allowlist bypass (nested SELECT INTO, PRAGMA write, comment injection), PDO resolver hijack, async query op spoofing, env variable tampering.

### P1 scenarios (next wave)

6. **S14** — Plugin hook order, timeout, WASM isolation, deregister (matrix §1230).
7. **S16** — Engine switch/fallback/warmup (matrix §1024) with Alpine-only integration binaries.
8. **S04** — QUIC/ACME: mark experimental; tests assert feature-flag off by default, not production-ready claims.
9. **S13** — OTLP export failure, `/metrics` under load, trace broken chain recovery.
10. **S19/S20** — Zero-copy shared memory mmap security (future v2 frame): Landlock mapping bounds, concurrent reader/writer races, TOCTOU on mmap regions.

When generating tests for a sector, output: `sector_id`, `matrix_rows`, `gate_command`, `stub_contract` (if any).

---

### Step 1: Classify the Target

| Target Type | Must-Test Categories |
|---|---|
| Pure function (no I/O, no side effects) | Core exhaustive (Phase 1) |
| Data type / struct (no behavior) | Core + Enum exhaustive (Phase 1 + §5) |
| I/O boundary function (file, net, DB) | Core + Security + Resource (Phase 1-4) |
| Concurrent / async component | Core + Concurrency + Resource (Phase 1, 3, 4) |
| Security-sensitive (auth, crypto, parse) | Core + Security exhaustive (Phase 1, 2) |
| Domain aggregate / business logic | Core + State machine + Integration (Phase 1, §5, §9) |
| Binary protocol / frame | Core + Security + Fuzzing + Protocol compliance |
| SQL/allowlist filter | Security exhaustive + injection bypass + all SQL dialect variants |
| Worker pool | Core + Concurrency + Resource + Stress + Lifecycle |
| Env-driven feature flag | Core + Security + Decision exhaustive + env tampering |

### Step 2: Prioritize by Risk Score

Generate tests in this priority order:

| Priority | Category | Risk Weight |
|---|---|---|
| P0 | Security (injection, overflow, auth bypass) | Critical |
| P0 | Panic / unwrap in library code | Critical |
| P0 | Binary protocol decode from untrusted input (fuzz target) | Critical |
| P0 | SQL allowlist bypass / write injection | Critical |
| P1 | Error handling (Result propagation) | High |
| P1 | Concurrency races / deadlocks | High |
| P1 | Worker pool lifecycle (spawn, recycle, exhaustion) | High |
| P2 | Boundary / edge cases | Medium |
| P2 | State machine transitions | Medium |
| P3 | Happy path (nominal flow) | Low |
| P3 | Serialization / deserialization | Low |

---

## Test Generation Protocol

Generate tests in ALL categories below. Do NOT skip. Do NOT summarize. Generate every pattern that applies.

### Phase 1: Core Exhaustive (Always Generate)

For EVERY function/type, generate:

| Category | Count | Details |
|---|---|---|
| Happy paths | All valid input combinations | Every enum variant, every type combination |
| Error paths | Every error variant | Match every arm of the error enum |
| Edge cases | ALL from input type table | See reference.md Input Type Matrix |
| Boundary values | At, below, above EVERY boundary | Zero, min, max, wrap, overflow |

### Phase 2: Security Exhaustive (If Input from External)

If the function accepts ANY input from network, file, user, or environment:

| Category | Count | Details |
|---|---|---|
| Injection | ALL vectors | SQL, command, path, XSS, LDAP, template, format string, binary opcode injection |
| Encoding attacks | ALL encodings | URL, base64, hex, unicode normalization (NFC/NFD/NFKC/NFKD), double-encode |
| Length attacks | ALL sizes | 0, 1, 255, 256, 1024, 4096, 65535, 65536, 1MB, 100MB, 1GB |
| Character attacks | ALL dangerous chars | Null bytes, control chars, RTL override, zero-width, combining chars |
| Binary protocol attacks | ALL frame corruptions | Magic mismatch, version skew, truncated length, length overflow, op out of range, payload type confusion |
| SQL injection | ALL SQL dialects | SELECT 1 OR 1=1, UNION SELECT, comment injection (--, #, /* */), stacked queries, time-based blind |

### Phase 3: Concurrency Exhaustive (If async/multi-thread)

| Category | Count | Details |
|---|---|---|
| Race conditions | ALL shared state paths | Read-read, read-write, write-write races |
| Deadlock | ALL lock orderings | A->B, B->A, circular chains |
| Starvation | ALL contention scenarios | Hot key, writer-priority, reader-priority |
| Memory ordering | ALL atomic orderings | Relaxed, Acquire, Release, AcqRel, SeqCst |
| Task cancellation | ALL cancellation points | Drop during await, panic in task, abort |
| Worker pool races | Idle queue concurrent push/pop | Worker return + recycle simultaneously |

### Phase 4: Resource Exhaustive (If manages resources)

| Category | Count | Details |
|---|---|---|
| Leaks | ALL resource types | FDs, memory, handles, connections, locks, threads, PHP child processes |
| Exhaustion | ALL limits | Max FDs, max memory, max connections, max threads, max workers |
| Cleanup | ALL failure points | Cleanup after panic, error, timeout, cancel, kill -9, SIGKILL PHP child |

---

## Naming Convention (Strict)

```rust
// Pattern: {module}_{function}_{input}_{expected}_{variant}
// Variant only when multiple tests for same scenario

// Happy paths
#[test]
fn parser_url_http_valid_returns_parsed() { }
#[test]
fn parser_url_https_valid_returns_parsed_with_tls() { }
#[test]
fn parser_url_ftp_valid_returns_parsed() { }

// Error paths - one per error variant
#[test]
fn parser_url_empty_returns_empty_error() { }
#[test]
fn parser_url_missing_scheme_returns_scheme_error() { }
#[test]
fn parser_url_invalid_port_returns_port_error() { }
#[test]
fn parser_url_oversized_returns_too_large_error() { }

// Edge cases - exhaustive
#[test]
fn parser_url_whitespace_only_returns_error() { }
#[test]
fn parser_url_null_byte_returns_error() { }
#[test]
fn parser_url_control_chars_returns_error() { }
#[test]
fn parser_url_unicode_nfc_returns_parsed() { }
#[test]
fn parser_url_unicode_nfd_returns_parsed() { }
#[test]
fn parser_url_rtl_override_returns_error() { }
#[test]
fn parser_url_zero_width_chars_returns_error() { }
#[test]
fn parser_url_emoji_in_path_returns_parsed() { }
#[test]
fn parser_url_idn_domain_returns_parsed() { }

// Security
#[test]
fn parser_url_sql_injection_returns_error_or_sanitized() { }
#[test]
fn parser_url_path_traversal_returns_error_or_sanitized() { }
#[test]
fn parser_url_double_encoded_traversal_returns_error() { }
#[test]
fn parser_url_format_string_returns_error() { }
```

---

## Arrangement Pattern (Strict)

```rust
#[test]
fn function_scenario_expected() {
    // === Arrange ===
    // Precondition setup
    let input = "...";
    let config = Config::builder()
        .max_size(1024)
        .timeout(Duration::from_secs(5))
        .build();

    // === Act ===
    let result = function_under_test(&input, &config);

    // === Assert ===
    // 1. Result type
    assert!(result.is_ok()); // or is_err()

    // 2. Success: validate ALL fields
    let value = result.unwrap();
    assert_eq!(value.field1, expected1);
    assert_eq!(value.field2, expected2);

    // 3. Error: validate variant AND message
    let err = result.unwrap_err();
    assert!(matches!(err, Error::SpecificVariant));
    assert!(err.to_string().contains("expected message"));

    // 4. Side effects (if any)
    assert_eq!(metrics.counter.load(), expected_count);

    // 5. Resource state (if any)
    assert_eq!(get_open_fds(), start_fds, "FD leak detected");
}
```

---

## Exhaustive Input Type Matrix

Generate ALL test cases listed for the input type:

| Type | Test Every |
|---|---|
| `&str` | "", " ", "\t", "\n", "\r\n", "a", "ab", null byte, all control chars (0x00-0x1F), DEL (0x7F), unicode BMP, unicode supplementary, emoji, RTL override chars (U+202E), LTR override chars (U+202A), zero-width space (U+200B), zero-width joiner (U+200D), combining chars, NFC form, NFD form, NFKC form, NFKD form, SQL injection patterns (10+), XSS patterns (10+), path traversal patterns (10+), format string patterns (%s, %n, {}), max length (1MB), max length + 1, valid but very long (10K chars) |
| `i8/i16/i32/i64/i128` | 0, 1, -1, MIN, MAX, MIN+1, MAX-1, overflow on add, overflow on mul, overflow on sub, wrapping behavior checked, negation of MIN (panic on two's complement) |
| `u8/u16/u32/u64/u128` | 0, 1, MIN, MAX, MAX+1 (wrap to 0), overflow on add, overflow on mul, overflow on sub, underflow on sub |
| `f32/f64` | 0.0, -0.0, 1.0, -1.0, NaN, +inf, -inf, MAX, MIN, denormalized (subnormal), epsilon, PI, subnormal to normal transition, precision loss, very small (1e-308), very large (1e+308) |
| `Vec<T>` | empty, 1 element, 2 elements, 100 elements, max capacity, max + 1, all duplicates, all unique, sorted, reverse sorted, interleaved |
| `String` (as owned) | Same as &str + UTF-8 boundary, invalid UTF-8 bytes, surrogate pairs, UTF-16 artifacts |
| `Option<T>` | None, Some(default T), Some(min T), Some(max T) |
| `Result<T, E>` | Ok(valid), Err(every E variant with realistic data) |
| `bool` | true, false, computed true, computed false |
| `Path/PathBuf` | "", ".", "..", "/", absolute, relative, UNC (Windows), symlink, broken symlink, exists, not exists, permission denied, read-only dir, network path, device path (/dev/null), special chars in name, very long path (>4096 chars on Linux, >260 on Windows) |
| `Duration` | 0, 1ns, 1ms, 1s, 1min, 1hr, 1day, MAX, sub-millisecond precision, overflow on add, overflow on mul |
| `IpAddr` | 0.0.0.0, 127.0.0.1, 255.255.255.255, private ranges (10.x, 172.16-31.x, 192.168.x), link-local (169.254.x.x), multicast, broadcast, IPv6 ::1, IPv6 ::, IPv6 link-local fe80::, IPv6 unique-local, IPv4-mapped IPv6 ::ffff:127.0.0.1, IPv6 multicast, invalid strings |
| `Url` | http, https, ftp, ftps, file, data, javascript, mailto, tel, about:blank, malformed (no scheme, no host), very long query string (>10K), IDN domains, punycode, very long path (>4K), fragment, userinfo with password |

---

## Concurrency Test Matrix

For EVERY shared resource, generate:

### Data Race Tests

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = N)]
async fn {resource}_{operation}_no_race()
// Run for N in [2, 4, 8, 16]

// Test matrix:
// 1. Multiple readers (no writers) - should all succeed
// 2. Single writer, no readers - should succeed
// 3. Writer + readers concurrently - no data corruption
// 4. Multiple writers concurrently - serialized correctly
// 5. Reader detects writer's change
// 6. Writer doesn't block on reader drop
```

### Deadlock Tests

```rust
#[tokio::test]
async fn {operation}_no_deadlock_{lock_order}()
// Run for EVERY permutation of lock acquisition order

// Test matrix:
// 1. A then B (normal order)
// 2. B then A (reverse order)
// 3. A then B then C (chain)
// 4. C then B then A (reverse chain)
// 5. Circular: A->B, B->C, C->A (deadlock scenario)
// 6. Each test runs 1000+ iterations
```

### Channel Tests

```rust
#[tokio::test]
async fn channel_{scenario}()

// Test matrix:
// 1. Send faster than consume (backpressure)
// 2. Consume faster than send (starvation prevention)
// 3. Multiple producers, single consumer (MPSC)
// 4. Single producer, multiple consumers (broadcast)
// 5. Multiple producers, multiple consumers (MPMC)
// 6. Producer drops mid-send (partial messages)
// 7. Consumer drops mid-receive (no leak)
// 8. Channel full (bounded, backpressure behavior)
// 9. Channel empty (unbounded, memory growth)
// 10. Send very large messages (memory pressure)
// 11. Send many tiny messages (allocation overhead)
```

---

## Security Test Matrix

For EVERY input boundary, generate ALL tests from security-tests.md plus:

### Unicode Normalization Attacks

```rust
// Test ALL 4 normalization forms
// NFC, NFD, NFKC, NFKD
// Especially for: filenames, URLs, user input

#[test]
fn input_nfc_vs_nfd_different() {
    let nfc = "\u{00E9}";       // e with acute (composed)
    let nfd = "e\u{0301}";      // e + combining acute (decomposed)
    assert_ne!(nfc, nfd);

    // If your code compares these, test both
    let result_nfc = process(nfc);
    let result_nfd = process(nfd);

    // They should behave identically OR be normalized first
    assert_eq!(result_nfc, result_nfd);
}
```

### RTL/LTR Override Attacks

```rust
#[test]
fn input_rtl_override_rejected_or_displayed_safely() {
    // U+202E RTL Override - makes text display right-to-left
    let input = "exe\u{202E}dfg.jpg"; // displays as "exejpg.gfd"
    let result = validate_filename(input);
    assert!(result.is_err(), "RTL override should be rejected");
}
```

### Homoglyph Attacks

```rust
#[test]
fn input_homoglyph_detected() {
    // Latin 'a' vs Cyrillic 'а' (look identical)
    let latin = "example.com";
    let cyrillic = "еxample.com"; // Cyrillic е
    assert_ne!(latin, cyrillic);

    // If your code compares domains/usernames, test this
    let result = validate_hostname(cyrillic);
    assert!(result.is_err(), "Homoglyph should be rejected");
}
```

### Binary Protocol Attack Surface (NEW: NEB1 frame)

```rust
// For EVERY binary protocol decoder (NEB1, IPC frames):

#[test]
fn frame_truncated_returns_error() {
    let truncated = b"NEB1\x01\x00\x00\x00"; // only header, no body
    assert!(decode_response(truncated).is_err());
}

#[test]
fn frame_bad_magic_rejected() {
    let frame = b"XXXX\x10\x00\x00\x00NEB1\x01\x03\x00\x00";
    assert!(frame_op(frame).is_err());
}

#[test]
fn frame_version_future_rejected() {
    // Craft a frame with version=255
    let mut frame = Vec::from(b"NEB1".to_vec());
    frame.push(255); // version
    frame.push(3);   // op
    frame.extend_from_slice(&[0, 0]); // pad
    // ... wrap with length prefix
    assert!(frame_op(&frame).is_err());
}

#[test]
fn frame_op_255_rejected() {
    // Valid header but unknown opcode
    // Must not panic, must return structured error
}

#[test]
fn frame_length_overflow_panics_prevented() {
    // u32::MAX length in outer wrapper with 8 bytes of data
    // Must not allocate 4GB, must fail fast
    let mut buf = Vec::new();
    buf.extend_from_slice(&u32::MAX.to_le_bytes());
    buf.extend_from_slice(b"short data");
    assert!(strip_wrapper(&buf).is_err());
}

#[test]
fn frame_nested_injection() {
    // Header JSON inside frame body contains SQL injection
    let headers = r#"{"Host": ["localhost'; DROP TABLE users;--"]}"#;
    // Must be treated as opaque JSON, not executed
}

#[test]
fn async_query_sql_injection_bypass_attempt() {
    // SQL that looks read-only but has side effects
    assert!(!is_readonly_sql("SELECT 1 INTO OUTFILE '/tmp/evil'"));
    assert!(!is_readonly_sql("PRAGMA writable_schema=1"));
    assert!(!is_readonly_sql("SELECT sql/* -- */"));
    assert!(!is_readonly_sql("SELECT 1; INSERT INTO x VALUES(1)"));
}
```

### SQL Allowlist Bypass Matrix (NEW: async_io)

Test `is_readonly_sql` against ALL known bypass techniques:

| Attack Vector | Example SQL | Expected `is_readonly_sql` |
|---|---|---|
| Stacked queries | `SELECT 1; DROP TABLE users` | **false** |
| SELECT INTO | `SELECT * INTO OUTFILE '/tmp/x'` | **false** |
| SELECT FOR UPDATE | `SELECT * FROM x FOR UPDATE` | **false** |
| PRAGMA write | `PRAGMA writable_schema=1` | **false** |
| CTE write | `WITH x AS (SELECT 1) INSERT INTO y SELECT * FROM x` | **false** (first word is WITH) — must also scan body |
| Comment injection | `SELECT/* comment */1` | **true** but comment may hide semicolon |
| Multiline injection | `SELECT\n1; DROP TABLE x` | **false** — newline between stmt and injection |
| Unicode SQL keyword | `ＳＥＬＥＣＴ 1` (fullwidth) | **false** — not ASCII SELECT |
| Lowercase bypass attempt | `select 1` | **true** — normalized to uppercase |
| Empty SQL | `""` | **false** |
| Whitespace-only | `"   "` | **false** |
| PRAGMA journal_mode | `PRAGMA journal_mode=WAL` | **true** — read-only pragma |
| EXPLAIN query | `EXPLAIN SELECT * FROM users` | **true** — read-only |
| Deeply nested SELECT | `SELECT (SELECT (SELECT 1))` | **true** — still read-only |

### Environment Variable Tampering (NEW: env-driven features)

```rust
// For every NUSA_* env var that controls security-sensitive behavior:

#[test]
fn env_nusa_async_io_malicious_value_uses_noop() {
    // Set NUSA_ASYNC_IO=DROP_TABLE or NUSA_ASYNC_IO=../../etc/passwd
    // Must fall through to NoopAsyncIoBridge, not crash or enable feature
    std::env::set_var("NUSA_ASYNC_IO", "DROP TABLE users");
    let bridge = bridge_from_env();
    assert_eq!(bridge.name(), "noop");
}

#[test]
fn env_nusa_async_sqlite_path_traversal_blocked() {
    // NUSA_ASYNC_SQLITE_PATH=../../../../etc/passwd
    // Must either reject or open in read-only mode within allowed dir
}

#[test]
fn env_nusa_embed_transport_invalid_uses_default() {
    // NUSA_EMBED_TRANSPORT=binary,frame_v2,evil
    // Must fall back to default (frame), not panic or enable unknown transport
}

#[test]
fn env_tampering_race() {
    // Change env var mid-request while worker is bootstrapped
    // Must not affect already-booted worker behavior
}
```

---

## Memory Test Matrix

### Leak Detection

```rust
#[test]
fn {resource}_no_leak_{scenario}()
// Run for EVERY resource type and scenario

// Test matrix:
// 1. Normal creation + drop
// 2. Creation + error + drop
// 3. Creation + panic + drop
// 4. Creation + partial use + drop
// 5. Creation in loop (N iterations, check no growth)
// 6. Creation + clone + drop both
// 7. Creation + move to thread + thread finishes
// 8. Creation + move to thread + thread panics
// 9. Creation + Arc + drop all clones
// 10. Creation + weak reference + strong dropped
```

### Allocation Pattern Tests

```rust
#[test]
fn {operation}_allocation_count_within_limit() {
    use std::alloc::System;
    let start = get_allocation_count();
    operation();
    let end = get_allocation_count();

    // Should not allocate more than expected
    assert!(end - start < MAX_ALLOWED_ALLOCS);
}
```

---

## Real-World Bug Pattern Tests

Generate tests for these common Rust bug patterns:

### Integer Overflow

```rust
#[test]
fn arithmetic_no_overflow_{operation}()
// For EVERY arithmetic operation in the code

#[test]
fn add_wraps_in_debug_mode() {
    let result = std::panic::catch_unwind(|| {
        let _: u8 = 255 + 1;
    });
    // In debug: panics. In release: wraps.
    // Test BOTH behaviors if your code does arithmetic
}

#[test]
fn checked_arithmetic_returns_none_on_overflow() {
    let result = u32::MAX.checked_add(1);
    assert!(result.is_none());
}
```

### Off-by-One Errors

```rust
#[test]
fn index_at_boundary_{collection}()
// For EVERY collection/array in the code

#[test]
fn slice_index_at_upper_boundary() {
    let arr = [1, 2, 3];
    assert_eq!(&arr[0..3], &[1, 2, 3]);      // valid
    // assert_eq!(&arr[0..4], ...);           // panic - test it
    let result = std::panic::catch_unwind(|| &arr[0..4]);
    assert!(result.is_err());
}
```

### Iterator Exhaustion

```rust
#[test]
fn iterator_exhausted_returns_none() {
    let mut iter = vec![1, 2, 3].into_iter();
    assert_eq!(iter.next(), Some(1));
    assert_eq!(iter.next(), Some(2));
    assert_eq!(iter.next(), Some(3));
    assert_eq!(iter.next(), None);
    // Calling next again should still be None (not panic)
    assert_eq!(iter.next(), None);
}
```

### HashMap Non-Determinism

```rust
#[test]
fn hash_map_iteration_order_not_relied_upon() {
    let mut map = HashMap::new();
    map.insert("a", 1);
    map.insert("b", 2);
    map.insert("c", 3);

    let keys: Vec<_> = map.keys().collect();
    // Keys order is NOT guaranteed
    // Test that your code doesn't depend on order
    assert_eq!(keys.len(), 3);
    assert!(keys.contains(&&"a"));
    assert!(keys.contains(&&"b"));
    assert!(keys.contains(&&"c"));
}
```

---

## Business Logic & State Machine Test Matrix

For EVERY state machine or business logic component, generate:

### State Transition Tests

```rust
#[test]
fn state_{from}_to_{to}_valid()
// For EVERY valid state transition

#[test]
fn state_{from}_to_{to}_invalid_rejected()
// For EVERY invalid state transition
```

Test matrix:
1. Every valid transition — should succeed
2. Every invalid transition — should be rejected at compile-time (type-state) or runtime (enum state)
3. Skip states — jumping from Draft to Published without Review
4. Reverse transitions — Published back to Draft
5. Self-transitions — already in target state, should be idempotent or no-op
6. Invalid terminal state operations — calling publish on already-published
7. Concurrent transitions — two threads trying to transition simultaneously
8. Transition after partial failure — rollback behavior

### Business Rule Tests

```rust
#[test]
fn business_rule_{rule_name}_{scenario}()
// For EVERY business rule in the domain
```

Test matrix:
1. Rule satisfied — operation succeeds
2. Rule violated by one unit — operation fails with specific error
3. Rule violated by maximum amount — operation fails
4. Rule at exact boundary — operation succeeds
5. Rule just above boundary — operation succeeds
6. Rule just below boundary — operation fails
7. Multiple rules conflict — priority behavior
8. Rule with empty input — default behavior
9. Rule with maximum input — performance and correctness
10. Rule mutation over time — if rules are configurable

### Invariant Tests

```rust
#[test]
fn invariant_{name}_maintained_after_{operation}()
// For EVERY invariant in the domain
```

Test matrix:
1. Invariant holds after valid operation
2. Invariant preserved after error
3. Invariant preserved after panic (drop cleanup)
4. Invariant preserved under concurrent access
5. Aggregate boundary — child entities can't violate parent invariant

### Workflow Tests

```rust
#[test]
fn workflow_{name}_{scenario}()
// For EVERY multi-step workflow
```

Test matrix:
1. Happy path — all steps in order
2. Missing step — skip required step, should fail
3. Wrong order — steps out of sequence, should fail
4. Partial workflow — stop mid-way, verify state
5. Retry failed step — idempotent or error
6. Cancel mid-workflow — cleanup behavior
7. Timeout mid-workflow — resource cleanup
8. Parallel workflows — no interference

---

## Decision Logic & Condition Coverage

For EVERY function with conditional logic, generate:

### Decision Table / Truth Table Tests

```rust
#[test]
fn logic_{condition_combo}_{expected_outcome}()
// For EVERY combination of boolean conditions
```

Test matrix:
1. All conditions true — expected outcome
2. All conditions false — expected outcome
3. Every single condition toggled — isolate each condition's effect
4. Every pair of conditions toggled — detect interaction bugs
5. Impossible combinations — document and assert unreachable

### Guard Clause / Precondition Tests

```rust
#[test]
fn guard_{precondition}_{violated_or_met}()
// For EVERY guard clause in the function
```

Test matrix:
1. Each guard clause violated — early return/error
2. Each guard clause met — continues to main logic
3. Multiple guards violated — first failing guard wins
4. Guard at boundary value — exactly at threshold
5. Guard just inside/outside boundary — off-by-one detection
6. Authorization guard — unauthorized, authorized, expired token, wrong role
7. Feature flag guard — enabled, disabled, partial rollout
8. Conditional compilation guard — `#[cfg(feature = "x")]` enabled/disabled

### Combinatorial / Pairwise Tests

```rust
#[test]
fn combo_{param_a}_{param_b}_{expected}()
// For functions with N independent inputs
```

Test matrix:
1. All valid combinations — if N is small enough (exhaustive)
2. Pairwise combinations — if N is large (each pair of values tested at least once)
3. Orthogonal arrays — systematic reduction for very large input spaces
4. Known bad combinations — documented failure cases from bugs/incidents

### Time-Dependent Logic Tests

```rust
#[test]
fn time_{scenario}_{condition}()
// For EVERY time-dependent behavior
```

Test matrix:
1. At exact expiry time — still valid or expired?
2. One tick before expiry — still valid
3. One tick after expiry — expired
4. Timezone boundary — DST transition, leap year, leap second
5. Clock skew — server clock vs client clock difference
6. Negative duration — should reject or treat as zero
7. Very large duration — overflow detection
8. Mocked time progression — advance time without waiting
9. Schedule boundary — cron expression edge cases, next execution calculation

### Event-Driven Logic Tests

```rust
#[test]
fn event_{scenario}_{ordering}()
// For EVERY event handler
```

Test matrix:
1. Events in correct order — expected behavior
2. Events in reverse order — idempotent or error
3. Duplicate events — idempotent (no double processing)
4. Missing events — state inconsistent, detection mechanism
5. Out-of-order events — buffer, reorder, or reject
6. Event replay — same events replayed produce same result
7. Concurrent events — same event from multiple sources
8. Event after timeout — stale event handling
9. Event storm — N events per second, no loss or duplication

### Fallback / Default Logic Tests

```rust
#[test]
fn fallback_{scenario}_{trigger}()
// For EVERY fallback path
```

Test matrix:
1. Primary available — uses primary, not fallback
2. Primary unavailable — uses fallback
3. Primary slow — timeout triggers fallback
4. Primary returns error — fallback with degraded result
5. Fallback also unavailable — error with both reasons
6. Fallback result correctness — degraded but still valid
7. Switch back to primary — recovery detection

### Rate Limiting Logic Tests

```rust
#[test]
fn rate_limit_{scenario}_{load}()
// For EVERY rate limiter
```

Test matrix:
1. Under limit — all requests pass
2. Exactly at limit — last request passes
3. One over limit — rejected with 429
4. Well over limit — all rejected, no resource waste
5. After window reset — counter resets, requests pass
6. Sliding window — partial window, requests proportional to elapsed time
7. Burst allowance — sudden burst within burst limit
8. Burst exceeded — rejected, normal rate resumes
9. Multiple tenants — each tenant's limit independent
10. Limit change at runtime — new limit takes effect immediately

---

## Stress Test Matrix

For EVERY component that handles requests, messages, or concurrent operations:

### Throughput Tests

```rust
#[tokio::test]
async fn component_throughput_{operations_per_second}()
// Test at: 100, 1K, 10K, 100K ops/sec
```

Test matrix:
1. Sustained throughput at target rate — no errors
2. Sustained throughput above target — graceful degradation or backpressure
3. P50, P95, P99 latency under load — within SLA
4. Throughput vs latency tradeoff — measure degradation curve

### Load Ramp-Up Tests

```rust
#[tokio::test]
async fn load_ramp_{component}_gradual_increase()
// Increase load by 10% every N seconds
```

Test matrix:
1. Gradual increase from 0% to 100% over 60 seconds — stable
2. Gradual increase to 200% — when does it break?
3. Gradual increase then hold at max — does it stabilize or degrade?
4. Ramp up, then ramp down — does it recover cleanly?

### Spike Load Tests

```rust
#[tokio::test]
async fn spike_load_{component}_{multiplier}x()
// Spike from baseline to N x baseline
```

Test matrix:
1. 2x spike for 10 seconds — no failures
2. 5x spike for 5 seconds — some failures acceptable, no corruption
3. 10x spike for 1 second — fail fast, recover quickly
4. Repeated spikes (every 30 seconds) — no resource accumulation
5. Spike during recovery — already degraded, then spike

### Endurance / Soak Tests

```rust
#[tokio::test]
async fn soak_{component}_{duration_minutes}_minutes()
// Run for extended periods: 5, 30, 60, 180 minutes
```

Test matrix:
1. Sustained load for 5 minutes — baseline stability
2. Sustained load for 30 minutes — memory stable (no leak)
3. Sustained load for 180 minutes — no gradual degradation
4. Check every 30 seconds: latency, throughput, memory, open FDs, thread count
5. No growth in: heap usage, connection pool size, queue depth, pending tasks

### Thundering Herd Tests

```rust
#[tokio::test]
async fn thundering_herd_{component}_after_{event}()
// Simulate simultaneous requests after recovery event
```

Test matrix:
1. All clients reconnect after server restart — no cascade failure
2. All clients request cache miss simultaneously — single computation, shared result
3. All clients timeout simultaneously — cleanup behavior
4. Stampede protection — only one does the work, others wait

### Backpressure Tests

```rust
#[tokio::test]
async fn backpressure_{component}_at_{limit}()
// Test behavior when limits are reached
```

Test matrix:
1. Queue full — new requests rejected with specific error (not dropped silently)
2. Rate limit hit — 429 or backpressure signal, not hanging
3. Connection pool exhausted — wait with timeout, not infinite block
4. Memory limit reached — OOM prevented, graceful degradation
5. CPU saturated — queue grows, latency increases, but no crash

### Degradation & Recovery Tests

```rust
#[tokio::test]
async fn degradation_{component}_under_{condition}()
// Test behavior under resource constraints
```

Test matrix:
1. High latency dependency — circuit breaker opens, fallback activates
2. Dependency returns errors — circuit breaker, retry, then open
3. Dependency slow then recovers — circuit half-open, probe, then close
4. Resource exhaustion then freed — recovers without restart
5. Partial failure (50% errors) — degrades gracefully, not all-or-nothing

---

## Deep Logic Test Matrix

For EVERY component requiring deep behavioral testing:

### Serialization/Deserialization Tests

```rust
#[test]
fn serialization_{format}_{scenario}()
// For EVERY serializer/deserializer
```

Test matrix:
1. Roundtrip — serialize then deserialize equals original
2. Valid minimal input — smallest valid structure
3. Valid maximal input — largest practical structure
4. Malformed input — truncated, extra fields, wrong types
5. Missing required fields — error with field name
6. Extra unknown fields — ignored (not rejected)
7. Schema evolution — old format read by new code (backward compatible)
8. Schema evolution — new format read by old code (forward compatible)
9. Special characters — unicode, null bytes, control chars in string fields
10. Nesting depth — deeply nested structures (stack overflow prevention)
11. Duplicate keys — first wins, last wins, or error (depending on format)
12. Empty vs null — distinguish empty collection from null/None

### Configuration Logic Tests

```rust
#[test]
fn config_{scenario}_{condition}()
// For EVERY configuration parser
```

Test matrix:
1. Defaults only — all defaults applied
2. File only — file overrides defaults
3. File + env — env overrides file
4. File + env + CLI — CLI overrides env
5. Invalid config file — clear error with line number
6. Missing config file — uses defaults, not crash
7. Hot-reload — change file, new values picked up
8. Hot-reload during active use — no corruption
9. Invalid env value — error with variable name
10. Empty config — defaults or error (depending on required fields)
11. Config with deprecated keys — warning + migration
12. Config precedence — document and verify override order

### Error Propagation Chain Tests

```rust
#[test]
fn error_chain_{layer}_{scenario}()
// For EVERY error propagation path
```

Test matrix:
1. Error at leaf — propagates to top with context
2. Error in middle layer — preserves inner context
3. Error converted — internal error mapped to user-facing error
4. Multiple errors in chain — only top-level returned, others logged
5. Error with source — `#[from]` preserves chain
6. Error context lost — verify context NOT lost (use `.context()`)
7. Timeout triggers error — proper error variant
8. Cancellation triggers error — distinguish from timeout

### State Restoration Tests

```rust
#[test]
fn state_restore_{scenario}()
// For EVERY component with persisted state
```

Test matrix:
1. Clean shutdown then restart — state restored correctly
2. Crash then restart — state recovered or reset
3. Partial write then restart — rollback or recovery
4. Corrupted state file — error + safe defaults
5. State version mismatch — migration or rejection
6. Concurrent access during restore — locked until ready
7. Large state — restore time within SLA

### Idempotency Tests

```rust
#[test]
fn idempotent_{operation}()
// For EVERY operation that should be idempotent
```

Test matrix:
1. Called once — expected result
2. Called twice — same result, no side effects doubled
3. Called N times (100+) — stable, no resource accumulation
4. Interrupted mid-operation, then retried — completes correctly
5. After partial failure — retry completes
6. Concurrent calls — serialized or merged correctly

### Cross-Boundary Tests

```rust
#[test]
fn boundary_{type}_{scenario}()
// For EVERY system boundary
```

Test matrix:
1. IPC boundary — malformed message rejected, valid message processed
2. FFI boundary — null pointer, invalid UTF-8, wrong struct size
3. WASM boundary — trap on invalid memory, correct value transfer
4. Network boundary — partial TCP frames, slow client, connection reset
5. File system boundary — permission denied, disk full, symlink attack
6. Memory boundary — buffer overflow attempt, use-after-free (safe Rust prevents)
7. Stdio boundary — PHP child sends malformed frame, partial write, stdout closed

### Protocol Compliance Tests

```rust
#[test]
fn protocol_{name}_{scenario}()
// For EVERY protocol implementation
```

Test matrix:
1. Valid minimal message — accepted
2. Valid maximal message — accepted
3. Malformed header — rejected with protocol error
4. Version mismatch — rejected with version error
5. Invalid checksum/signature — rejected
6. Timeout mid-stream — graceful disconnect
7. Unexpected message type — rejected or ignored per spec
8. Flow control — respects window/buffer limits

### Lifecycle Deep Edge Cases

```rust
#[test]
fn lifecycle_{scenario}()
// For EVERY type with non-trivial lifecycle
```

Test matrix:
1. Partial initialization then drop — no double-free, no leak
2. Clone then drop both — ref count zero, resources freed
3. Arc strong dropped, weak alive — weak detects upgrade failure
4. Move to thread, thread panics — cleanup happens
5. Send across thread boundary, receiver drops — no leak
6. Self-referential struct drop — no use-after-free
7. Recursive type deep nesting — stack overflow prevention

### Property-Based Testing Patterns

```rust
#[test]
fn property_{name}()
// For EVERY invariant that should always hold
```

Test matrix:
1. Invariant holds after any valid operation
2. Invariant holds after any sequence of operations (N=100)
3. Invariant holds under concurrent access
4. Shrinking — when test fails, minimal failing case found
5. Commutativity — A then B equals B then A (if applicable)
6. Associativity — (A op B) op C equals A op (B op C) (if applicable)
7. Inverse — A then inverse(A) returns to original state

### Graceful Degradation Tests

```rust
#[test]
fn degradation_{component}_{failure}()
// For EVERY external dependency
```

Test matrix:
1. Primary dependency down — fallback works
2. All dependencies down — safe error, no crash
3. Slow dependency — timeout, partial results returned
4. Wrong data from dependency — validation catches it
5. Intermittent failures — system stabilizes (not flapping)

### Platform-Specific Tests

```rust
#[test]
#[cfg(target_os = "linux")]
fn linux_{scenario}()

#[test]
#[cfg(target_os = "windows")]
fn windows_{scenario}()
```

Test matrix:
1. Path separators — `/` vs `\`
2. File permissions — Unix vs Windows ACLs
3. Signal handling — SIGTERM, SIGKILL (Unix only)
4. FFI calling conventions — cdecl, stdcall differences
5. Line endings — `\n` vs `\r\n`
6. Case sensitivity — file names on Linux vs Windows

---

## PHP-Rust Runtime Integration Test Matrix

For EVERY component in the PHP-Rust runtime architecture:

### PHP Engine Integration Tests

```rust
#[test]
fn engine_{type}_{scenario}()
// For EVERY engine type: ffi, wasm, child, embed
```

Test matrix:
1. FFI engine — PHP ZTS embed initialization
2. FFI engine — PHP script execution via FFI
3. FFI engine — concurrent FFI calls (thread safety)
4. FFI engine — PHP error propagation to Rust
5. FFI engine — memory leak detection after N executions
6. WASM engine — wasmtime sandbox creation
7. WASM engine — PHP WASI file system access
8. WASM engine — sandbox escape attempt (security)
9. WASM engine — WASM trap handling (division by zero, OOB)
10. WASM engine — WASM module hot-reload
11. Child engine — PHP process spawn
12. Child engine — STDIN/STDOUT/STDERR communication
13. Child engine — PHP process kill and cleanup
14. Child engine — PHP process crash detection
15. Child engine — zombie process prevention
16. Embed engine (S19) — PHP stdio daemon spawn over stdio
17. Embed engine (S19) — NEB1 frame encode/decode roundtrip
18. Embed engine (S19) — JSON transport fallback when NUSA_EMBED_TRANSPORT unset
19. Embed engine (S19) — PHP daemon crash mid-request, pool detects
20. Engine switching — FFI → WASM → Child → Embed without gateway change
21. Engine fallback — FFI fails, fallback to child
22. Engine warmup — cold start vs warm start latency

### Embed Worker Pool Tests (S19 — NEW)

```rust
#[test]
fn embed_pool_{scenario}()
// For EVERY embed pool behavior
```

Test matrix:
1. Pool creation — N workers spawned and booted
2. Pool shutdown — all workers killed, no zombie PHP processes
3. Worker checkout — idle_queue pop returns valid worker index
4. Worker return — worker_id pushed back to idle_queue
5. Idle queue duplicate prevention — same worker_id not pushed twice
6. Pool exhausted — no idle worker, returns NoIdleWorker error
7. Worker recycle by request count — after max_requests, worker recycled
8. Worker recycle with warm standby — standby replaces without spawn latency
9. Worker recycle without standby — spawn new PHP daemon inline
10. Standby replenishment — standby count maintained after consumption
11. Pool not_ready before init — handle_http_request returns NotReady
12. Pool metrics — total_handled, total_errors, active_workers accurate
13. Worker error — failed request increments total_errors, worker returned to queue
14. Concurrent pool access — M threads requesting N workers, no idle_queue corruption
15. Worker stdio pipe broken — detected as error, triggers recycle
16. Embed worker bootstrap fails — pool fails closed, not partially initialized
17. Pool with async_io bridge — bridge passed to every worker on spawn
18. Embed pool + circuit breaker — worker errors count toward breaker threshold
19. Pool max memory_mb recycle trigger — RSS monitoring (post-GA feature)
20. Pool transport mismatch — NUSA_EMBED_TRANSPORT changed mid-flight, stable behavior

### Octane Worker Lifecycle Tests

```rust
#[test]
fn worker_{scenario}()
// For EVERY worker lifecycle event
```

Test matrix:
1. Worker pool creation — N workers spawned
2. Worker pool shutdown — graceful drain
3. Worker request dispatch — request reaches worker
4. Worker response return — response reaches gateway
5. Worker state reset between requests — no cross-request pollution
6. Worker recycle — after N requests, worker replaced
7. Worker recycle — during active request, drain first
8. Worker health check — alive worker passes
9. Worker health check — dead worker detected and replaced
10. Worker panic — PHP panic inside worker, pool recovers
11. Worker memory leak — detect and recycle
12. Worker timeout — slow PHP request, timeout triggers
13. Worker pool resize — scale up under load
14. Worker pool resize — scale down when idle
15. Worker handshake — protocol version match
16. Worker handshake — version mismatch rejection
17. Worker concurrent — M requests to N workers (M > N, queuing)
18. Worker starvation — no idle worker, request waits with timeout

### Request Flow End-to-End Tests

```rust
#[test]
fn request_flow_{method}_{scenario}()
// For EVERY request path through the system
```

Test matrix:
1. GET request → Rust Gateway → PHP Engine → Response
2. POST request with body → PHP receives body correctly
3. Request with headers → PHP receives headers (RequestContext builder)
4. Request with query string → PHP receives query params
5. Request with cookies → PHP receives cookies
6. Request with file upload → PHP receives multipart data
7. Request with large body (>1MB) → streaming, no memory spike
8. Request with slow client → backpressure, no resource waste
9. Request timeout → client disconnects, PHP cancelled
10. Request → middleware chain → all middlewares execute in order
11. Request → circuit breaker open → 503 returned immediately
12. Request → rate limit exceeded → 429 returned
13. Request → tenant extraction → correct tenant context
14. Request → trace context → W3C TraceContext propagated to PHP
15. Concurrent requests → no cross-request data leakage
16. Sequential requests → same worker, state properly reset

### PHP State Isolation Tests

```rust
#[test]
fn state_isolation_{scenario}()
// For EVERY type of PHP global state
```

Test matrix:
1. Global variables — reset between requests
2. Static variables — reset between requests
3. Super-globals ($_GET, $_POST, $_SERVER) — cleared between requests
4. Include cache — cleared between requests
5. OpCache — shared (correct) vs per-request (correct behavior)
6. PHP extensions state — reset between requests
7. Session data — isolated per session ID
8. Memory usage — no growth across N requests
9. Open file handles — closed between requests
10. Database connections — pooled correctly, not leaked
11. Object instances — destroyed between requests
12. Class definitions — cached correctly (not reloaded each request)

### IPC Protocol Tests

```rust
#[test]
fn ipc_{scenario}()
// For EVERY IPC protocol behavior
```

Test matrix:
1. Frame encoding — correct binary format
2. Frame decoding — correct message extraction
3. Framing — partial frame received, waits for complete
4. Framing — multiple frames in single TCP packet
5. Framing — frame boundary alignment
6. Message serialization — rmp-serde roundtrip
7. Message serialization — large message (truncation prevention)
8. Message deserialization — malformed message rejected
9. Flow control — backpressure when receiver slow
10. Flow control — sender rate limiting
11. Error recovery — connection drop, reconnect and resume
12. Error recovery — corrupted frame, resync
13. Heartbeat — alive connection detected
14. Heartbeat — dead connection detected (timeout)
15. Protocol version — mismatch detection
16. Message ordering — FIFO preserved
17. Message priority — high priority processed first (if applicable)
18. Zero-copy — large payload transferred without copy (if applicable)

### NEB1 Frame Protocol Tests (S19 — NEW)

```rust
#[test]
fn neb1_frame_{scenario}()
// For EVERY NEB1 frame behavior
```

Test matrix:
1. Bootstrap frame encode → decode op = OP_BOOTSTRAP
2. ACK frame decode → returns Ok for valid ack
3. Request frame encode/decode roundtrip (method, URI, headers, body)
4. Response frame decode → status, headers, body extracted
5. Error frame decode → message string extracted
6. AsyncQuery frame encode/decode roundtrip (SQL string)
7. AsyncResult scalar encode/decode roundtrip
8. AsyncResult JSON encode/decode roundtrip
9. AsyncResult error encode/decode roundtrip
10. Frame too short (< 4 bytes outer length) → error
11. Frame magic mismatch ("XXXX" instead of "NEB1") → error
12. Frame version mismatch (version 0, 2, 255) → error
13. Frame unknown op (200) → error
14. Frame truncated (declared length > actual) → error
15. Frame length = u32::MAX with tiny body → no panic, fast fail
16. Frame inner length overflow → wrapper detects mismatch
17. Request method > 65535 bytes → encode returns error
18. Request URI > 4GB → encode returns error
19. Headers JSON deserialize failure → structured error
20. AsyncQuery SQL contains null bytes → passed through (Rust doesn't validate)
21. AsyncResult unknown kind byte (2, 255) → error
22. AsyncResult scalar with 4 bytes instead of 8 → EOF error
23. Frame with embedded SQL injection in header JSON → treated as opaque
24. NEB1 version negotiation — PHP sends v0, Rust rejects
25. Concurrent frame decode from same buffer → no data race

### VFS (Virtual File System) Tests

```rust
#[test]
fn vfs_{scenario}()
// For EVERY VFS operation
```

Test matrix:
1. File read within root — succeeds
2. File read outside root — blocked (path traversal prevention)
3. File read with `..` traversal — blocked
4. File read with symlink outside root — blocked
5. File read with encoded traversal (`%2e%2e`) — blocked
6. File read with Unicode normalization attack — blocked
7. File write within root — succeeds
8. File write outside root — blocked
9. Tenant isolation — tenant A cannot access tenant B files
10. Tenant isolation — shared directories correctly scoped
11. File permissions — read-only directory write attempt blocked
12. File not found — correct error, not crash
13. Large file read — streaming, no memory spike
14. Concurrent file access — no corruption
15. File modification during request — consistent snapshot or error
16. VFS hot-reload — file watch triggers reload

### Multi-Tenant Isolation Tests

```rust
#[test]
fn tenant_{scenario}()
// For EVERY tenant isolation boundary
```

Test matrix:
1. Tenant A request → tenant A context only
2. Tenant B request → tenant B context only
3. Tenant A cannot access tenant B resources
4. Tenant rate limit — tenant A limit independent of tenant B
5. Tenant resource quota — tenant A quota enforced separately
6. Tenant config — tenant A config doesn't affect tenant B
7. Tenant VFS root — isolated per tenant
8. Tenant worker — shared pool, tenant context isolated
9. Tenant metrics — per-tenant metrics collected
10. Tenant trace — per-tenant trace context
11. Tenant creation — new tenant registered correctly
12. Tenant deletion — tenant resources cleaned up
13. Tenant concurrent — N tenants, N requests, no cross-talk
14. Tenant missing from registry — 404 or default tenant

### Hot Reload Mechanism Tests

```rust
#[test]
fn hot_reload_{component}_{scenario}()
// For EVERY hot-reload target
```

Test matrix:
1. Config file change — ArcSwap picks up new config
2. Config file change — in-flight requests use old config
3. Config file change — new requests use new config
4. Config file change — invalid config rejected, old config retained
5. Config file change — file deleted, defaults used
6. Config file change — file recreated, config restored
7. PHP file change — file watch detects change
8. PHP file change — worker recycled with new code
9. PHP file change — in-flight requests complete with old code
10. Plugin change — plugin reloaded without restart
11. Hot reload under load — no request dropped during reload
12. Hot reload race — two changes simultaneously, both applied
13. Hot reload memory — old config dropped, no leak
14. Hot reload notification — reload event logged

### Plugin/Hook System Tests

```rust
#[test]
fn plugin_{hook_type}_{scenario}()
// For EVERY plugin hook
```

Test matrix:
1. Pre-exec hook — runs before PHP execution
2. Pre-exec hook — hook error fails request gracefully
3. Post-exec hook — runs after PHP execution
4. Post-exec hook — hook error logs, request still succeeds
5. Plugin WASM sandbox — plugin execution isolated
6. Plugin WASM sandbox — plugin crash doesn't affect main process
7. Plugin version compatibility — v1 plugin on v2 runtime
8. Plugin registration — plugin registered at startup
9. Plugin registration — plugin registered at runtime (hot-load)
10. Plugin deregistration — plugin removed, hooks removed
11. Plugin order — hooks execute in registered order
12. Plugin timeout — slow plugin timeout, skipped
13. Plugin metrics — plugin execution time tracked
14. Plugin error isolation — one plugin error doesn't kill others

### Laravel Compatibility Tests

```rust
#[test]
fn laravel_{feature}_{scenario}()
// For EVERY Laravel/Octane feature
```

Test matrix:
1. FPM mode — PHP-FPM compatible request handling
2. Octane mode — Octane-compatible worker protocol
3. Laravel session — session start, read, write, destroy
4. Laravel session — session persistence across requests
5. Laravel queue — queue job dispatch (if applicable)
6. Laravel cache — cache read/write through runtime
7. Laravel database — DB connection pooling, transaction handling
8. Laravel events — event dispatch through runtime
9. Laravel routing — route matching, parameter binding
10. Laravel middleware — middleware chain execution
11. Laravel Artisan — CLI command execution (if applicable)
12. Laravel testing — Laravel test suite passes on Nusa runtime
13. Laravel octane contract — all Octane interfaces implemented
14. Laravel version compatibility — Laravel 10, 11, 12

### Async I/O Bridge Tests (S20 — NEW)

```rust
#[test]
fn async_io_{scenario}()
// For EVERY async I/O bridge behavior
```

Test matrix:
1. NoopAsyncIoBridge — enabled() returns false
2. NoopAsyncIoBridge — select_one() returns NotSupported
3. SpikeSqliteAsyncIoBridge — enabled() returns true
4. SpikeSqliteAsyncIoBridge — select_one() returns 1
5. SpikeSqliteAsyncIoBridge — readonly SELECT returns rows
6. SpikeSqliteAsyncIoBridge — INSERT returns DisallowedSql
7. SpikeSqliteAsyncIoBridge — DELETE returns DisallowedSql
8. SpikeSqliteAsyncIoBridge — UPDATE returns DisallowedSql
9. SpikeSqliteAsyncIoBridge — multi-statement "SELECT 1; DROP TABLE" returns DisallowedSql
10. SpikeSqliteAsyncIoBridge — SELECT INTO returns DisallowedSql
11. SpikeSqliteAsyncIoBridge — SELECT FOR UPDATE returns DisallowedSql
12. SpikeSqliteAsyncIoBridge — PRAGMA read-only returns Rows
13. SpikeSqliteAsyncIoBridge — EXPLAIN returns Rows
14. SpikeSqliteAsyncIoBridge — WITH CTE SELECT returns Rows
15. bridge_from_env() with NUSA_ASYNC_IO=stub → SpikeSqliteAsyncIoBridge
16. bridge_from_env() with NUSA_ASYNC_IO=1 → NoopAsyncIoBridge
17. bridge_from_env() with NUSA_ASYNC_IO unset → NoopAsyncIoBridge
18. bridge_from_env() with NUSA_ASYNC_IO=malicious → NoopAsyncIoBridge (fail-safe)
19. async_io_requested_from_env() — "1", "true", "stub" all true
20. async_io_stub_from_env() — only "stub" true
21. SpikeSqliteAsyncIoBridge — shared connection across concurrent queries (Mutex contention)
22. SpikeSqliteAsyncIoBridge — query timeout behavior (spawn_blocking)
23. SpikeSqliteAsyncIoBridge — NUSA_ASYNC_SQLITE_PATH with valid file
24. SpikeSqliteAsyncIoBridge — NUSA_ASYNC_SQLITE_PATH with missing file → error
25. SpikeSqliteAsyncIoBridge — in-memory fallback when no path set
26. first_scalar_from_rows — empty array returns NotSupported
27. first_scalar_from_rows — non-numeric value returns NotSupported
28. sqlite_value_to_json — NULL, Integer, Real, Text, Blob all covered
29. async_io — blob value serialized as "<blob N bytes>" not raw bytes

### PHP AsyncIo.php Tests (S20 — NEW)

```php
// AsyncIo::isReadonlySql — mirror of Rust is_readonly_sql
// Must test both sides produce identical results (contract test)

#[test]
fn async_io_readonly_sql_php_vs_rust_parity() {
    // SELECT 1, WITH CTE, PRAGMA, EXPLAIN → both true
    // INSERT, UPDATE, DELETE, SELECT;DROP → both false
    // Verify parity for 20+ SQL patterns
}
```

Test matrix:
1. AsyncIo::enabled() — NUSA_ASYNC_IO=stub + NUSA_EMBED_TRANSPORT=frame → true
2. AsyncIo::enabled() — NUSA_ASYNC_IO=stub + frame unset → false
3. AsyncIo::enabled() — NUSA_ASYNC_IO unset → false
4. AsyncIo::isReadonlySql("SELECT 1") → true
5. AsyncIo::isReadonlySql("INSERT INTO x") → false
6. AsyncIo::isReadonlySql("SELECT 1; DROP TABLE") → false (semicolon check)
7. AsyncIo::isReadonlySql("  select count(*) from users ") → true (trim + case)
8. AsyncIo::isReadonlySql("SELECT * INTO OUTFILE '/tmp/x'") → false
9. AsyncIo::isReadonlySql("SELECT * FROM x FOR UPDATE") → false
10. AsyncIo::isReadonlySql("PRAGMA journal_mode") → true
11. AsyncIo::isReadonlySql("EXPLAIN SELECT * FROM x") → true
12. AsyncIo::isReadonlySql("") → false (empty)
13. AsyncIo::isReadonlySql("   ") → false (whitespace only)
14. AsyncIo::bootstrap — registers resolver when enabled
15. AsyncIo::bootstrap — no-op when not enabled
16. AsyncIo::registerSqliteResolver — idempotent (called twice = once)
17. AsyncIo::publishSqlitePath — sets NUSA_ASYNC_SQLITE_PATH from config
18. AsyncIo::publishSqlitePath — no-op when file doesn't exist
19. NusaAsyncSqliteConnection::select — routes to async bridge when enabled
20. NusaAsyncSqliteConnection::select with bindings — falls back to blocking PDO

### Security Hardening Tests

```rust
#[test]
fn security_{mechanism}_{scenario}()
// For EVERY security mechanism
```

Test matrix:
1. Landlock — filesystem access restricted to allowed paths
2. Landlock — network access restricted (if configured)
3. Landlock — Landlock denied syscall returns correct error
4. Seccomp-BPF — allowed syscalls pass
5. Seccomp-BPF — denied syscalls blocked (SIGSYS)
6. Seccomp-BPF — Seccomp filter loaded correctly
7. Circuit breaker — opens after N failures
8. Circuit breaker — half-open probe after timeout
9. Circuit breaker — closes after successful probe
10. Circuit breaker — per-tenant circuit breaker isolation
11. Resource guard — backpressure triggers at threshold
12. Resource guard — timeout kills slow operation
13. Resource guard — request size limit enforced
14. Unsafe code — `// SAFETY:` comments present on all unsafe blocks
15. Unsafe code — geiger scan shows zero unexpected unsafe
16. SBOM — dependency list generated correctly
17. SLSA provenance — build attestation correct
18. Embed NEB1 frame — malformed binary from PHP child doesn't panic (S19)
19. Embed code_dir path traversal — bootstrap with "../../../../etc" rejected (S19)
20. Async SQL allowlist — all known bypass vectors blocked (S20)

### Docker/Deployment Tests

```rust
#[test]
fn deployment_{scenario}()
// For EVERY deployment scenario
```

Test matrix:
1. Alpine musl build — compiles and runs
2. Multi-stage build — PHP ZTS correctly embedded
3. Docker run — health check passes
4. Docker run — ready check passes
5. Docker run — environment variable override
6. Docker run — volume mount for Laravel app
7. Docker run — port mapping correct
8. Docker run — signal handling (SIGTERM graceful shutdown)
9. Docker run — no root user (security)
10. Docker compose — runtime + Laravel app + DB
11. Cross-compile — Linux binary built on any host
12. Binary size — stripped binary within size limit
13. Startup time — cold start within SLA
14. Memory footprint — resident memory within limit

### PHP Driver Package Tests

```rust
#[test]
fn driver_{scenario}()
// For EVERY PHP driver component
```

Test matrix:
1. composer.json — valid JSON, correct dependencies
2. composer.json — version constraint matches Nusa runtime
3. Worker script — executes without error
4. Worker script — connects to Rust runtime
5. Worker script — handles shutdown signal
6. Driver installation — `composer require` succeeds
7. Driver installation — auto-discovery works
8. Driver version — matches Nusa runtime version
9. Driver upgrade — old driver on new runtime (compatibility)
10. Driver downgrade — new driver on old runtime (compatibility)
11. Embed daemon — nusa_embed_daemon.php runs under PHP 8.5
12. Embed daemon — FrameCodec.php handles NEB1 correctly
13. Embed AsyncIo.php — sqlite resolver registered before kernel boot

### Observability Tests

```rust
#[test]
fn observability_{type}_{scenario}()
// For EVERY observability component
```

Test matrix:
1. Trace context extraction — W3C TraceContext header parsed
2. Trace context injection — trace context added to response
3. Trace context propagation — Rust → PHP → Rust trace chain
4. Trace context missing — new trace generated
5. Trace context invalid — ignored, new trace generated
6. OTLP trace export — traces sent to collector
7. OTLP trace export — export failure doesn't block request
8. Prometheus metrics — request count correct
9. Prometheus metrics — latency histogram accurate
10. Prometheus metrics — per-tenant metrics isolated
11. Prometheus metrics — metric names follow conventions
12. JSON logs — structured log with trace_id, tenant_id
13. JSON logs — error logs include stack trace
14. JSON logs — log level filter respected
15. Health probe — /health returns OK when running
16. Health probe — /ready returns READY when initialized
17. Health probe — /ready returns NOT_READY when initializing
18. Health probe — /health returns FAIL when degraded
19. Metrics endpoint — /metrics returns Prometheus format
20. Observability under load — metrics/trace export doesn't degrade performance

---

## Advanced Runtime Test Matrix

Additional testing categories for production-grade runtime validation:

### Benchmark Regression Tests

```rust
#[bench]
fn bench_{component}_{scenario}(b: &mut Bencher)
// For EVERY performance-critical path
```

Test matrix:
1. Baseline — establish performance baseline for each operation
2. P50/P95/P99 latency — within SLA for each endpoint
3. Allocation count — no unexpected allocations per request
4. Memory fragmentation — stable over N iterations
5. Throughput ceiling — max requests/sec before degradation
6. Cold start — first request latency (no warmup)
7. Warm start — steady-state request latency (after 1000 requests)
8. CPU-bound operation — throughput proportional to CPU cores
9. I/O-bound operation — throughput proportional to I/O capacity
10. Benchmark regression — new code not slower than baseline by >5%

### Fuzzing Tests

```rust
// Use cargo-fuzz with libfuzzer-sys
cargo fuzz run {target}
```

Test matrix:
1. IPC message fuzzer — arbitrary binary input to IPC decoder
2. HTTP request fuzzer — arbitrary HTTP input to gateway
3. Config file fuzzer — arbitrary TOML to config parser
4. PHP response fuzzer — arbitrary PHP output to deserializer
5. URL/query fuzzer — arbitrary URL to router
6. NEB1 frame fuzzer (S19) — arbitrary binary to NEB1 decoder (frame_op, decode_response, decode_async_query, decode_async_result, decode_error)
7. SQL allowlist fuzzer (S20) — arbitrary strings to is_readonly_sql
8. Crash triage — each crash minimized to smallest reproducer
9. Coverage-guided — fuzzer explores new code paths
10. Dictionary-guided — fuzzer uses protocol keywords for smarter input
11. Seed corpus — real-world inputs used as starting seeds
12. Timeout — fuzzer doesn't hang on any input

### Long-Running Stability Tests

```rust
#[test]
fn stability_{scenario}_{duration}()
// Run for extended periods: 1hr, 6hr, 24hr, 7d
```

Test matrix:
1. 1 hour at 100 req/s — no memory growth
2. 6 hours at variable load — no gradual degradation
3. 24 hours — file descriptor count stable
4. 24 hours — thread count stable
5. 24 hours — connection pool size stable
6. 7 days — no zombie processes
7. 7 days — no stale locks
8. 7 days — log file rotation works correctly
9. 7 days — metrics endpoint still accurate
10. 7 days — no config drift from hot-reload bugs

### Chaos Engineering Tests

```rust
#[test]
fn chaos_{failure}_{scenario}()
// Random failure injection
```

Test matrix:
1. Random worker death — pool detects and replaces
2. Random PHP crash — worker recovers, no request loss
3. Random config change — invalid config rejected, no crash
4. Random network partition — circuit breaker opens, graceful degradation
5. Random disk full — error returned, not crash, cleanup recovers
6. Random OOM kill — process restarts, state recovered or reset
7. Random clock skew — time-dependent logic handles gracefully
8. Random signal — SIGTERM/SIGINT graceful shutdown
9. Random CPU spike — backpressure activates, no cascade failure
10. Random DNS failure — external service resolution recovers
11. Random file corruption — corrupted files detected, not loaded
12. Random rate limit trigger — legitimate requests not affected
13. Embed: PHP daemon killed mid-request — pool detects, recycles, retries
14. Embed: stdio pipe half-closed — graceful error, not panic
15. Async I/O: SQLite connection corrupted mid-query — error propagation

### Memory Profiling Tests

```rust
#[test]
fn memory_{scenario}()
// Memory behavior validation
```

Test matrix:
1. Peak memory — single request doesn't exceed limit
2. Steady-state memory — after 10K requests, memory stable
3. Allocation pattern — no allocation hotspots
4. Large object handling — large request/response handled without OOM
5. Memory fragmentation — no fragmentation after mixed-size allocations
6. Arc reference count — no leaked Arc clones
7. Buffer reuse — buffers returned to pool, not reallocated
8. String interning — repeated strings deduplicated (if applicable)
9. Cache eviction — LRU/TTL eviction frees memory
10. Memory limit enforcement — process stops accepting when limit reached

### WebSocket/Realtime Tests

```rust
#[test]
fn websocket_{scenario}()
// For EVERY WebSocket behavior
```

Test matrix:
1. Connection establishment — handshake completes
2. Message send — server → client message received
3. Message receive — client → server message processed
4. Connection persistence — connection survives idle timeout
5. Connection drop — reconnection with session recovery
6. Message ordering — FIFO delivery guaranteed
7. Broadcast — message fanned out to N subscribers
8. Backpressure — slow subscriber doesn't block others
9. Binary message — binary frame sent and received
10. Close handshake — graceful close with code/reason
11. Abnormal close — network drop detected
12. Rate limiting — message rate limited per connection
13. Connection limit — max connections enforced
14. Ping/Pong — heartbeat keeps connection alive
15. Large message — message size limit enforced

### ACME/TLS Renewal Tests

```rust
#[test]
fn tls_{scenario}()
// For EVERY TLS/ACME behavior
```

Test matrix:
1. Certificate auto-renewal — renewed 30 days before expiry
2. Certificate expiry detection — monitoring alerts
3. Graceful cert swap — in-flight connections not dropped
4. ACME challenge — HTTP-01 challenge served correctly
5. ACME challenge — DNS-01 challenge completed (if applicable)
6. ACME rate limit — respects Let's Encrypt rate limits
7. ACME failure — retry with backoff on failure
8. TLS 1.3 negotiation — client supports TLS 1.3
9. TLS fallback — rejects TLS 1.2 (if configured)
10. Certificate chain — full chain served (leaf + intermediate)
11. OCSP stapling — OCSP response attached (if enabled)
12. HSTS header — Strict-Transport-Security served
13. SNI — correct certificate for requested hostname

### QUIC/HTTP3 Tests

```rust
#[test]
fn quic_{scenario}()
// For EVERY QUIC behavior
```

Test matrix:
1. Connection establishment — QUIC handshake completes
2. 0-RTT resumption — repeat connection faster
3. Connection migration — IP change doesn't break connection
4. Stream multiplexing — multiple streams concurrent
5. Stream prioritization — priority headers respected
6. Flow control — stream/connection level flow control
7. Connection close — graceful QUIC close
8. Stateless reset — invalid packet triggers reset
9. Version negotiation — client/server version match
10. Fallback to HTTP/2 — QUIC unavailable, HTTP/2 works
11. ALPN negotiation — h3 protocol negotiated
12. Zero-copy QUIC send — large payload sent efficiently (if applicable)

### Static File Serving Tests

```rust
#[test]
fn static_file_{scenario}()
// For EVERY static file behavior
```

Test matrix:
1. File exists — served with correct content-type
2. File not found — 404 returned
3. Directory listing — disabled (security) or enabled (if configured)
4. Range request — byte range served correctly (video streaming)
5. ETag — ETag header generated correctly
6. If-None-Match — 304 returned for matching ETag
7. If-Modified-Since — 304 returned for unchanged file
8. Gzip compression — .gz served when client accepts
9. Brotli compression — .br served when client accepts
10. Cache-Control — max-age, no-cache, immutable headers set
11. Content-Disposition — attachment vs inline
12. Large file — streaming, no memory spike
13. Symlink — followed (safe) or blocked (security)
14. Hidden files — .git, .env blocked
15. Path traversal — `..` blocked
16. MIME type sniffing — X-Content-Type-Options: nosniff
17. CORS — static files respect CORS policy

### Database/External Service Integration Tests

```rust
#[test]
fn external_{service}_{scenario}()
// For EVERY external service integration
```

Test matrix:
1. Connection pool — pool creation with N connections
2. Connection pool — pool exhaustion, queuing behavior
3. Connection pool — connection recycling after N uses
4. Transaction — commit succeeds
5. Transaction — rollback on error
6. Transaction — deadlock detection and resolution
7. Query timeout — slow query cancelled
8. Connection drop — pool reconnects automatically
9. Connection health check — dead connection removed
10. Retry — transient failure retried with backoff
11. Circuit breaker — service down, breaker opens
12. Circuit breaker — service recovered, breaker closes

### Caching Layer Tests

```rust
#[test]
fn cache_{scenario}()
// For EVERY caching behavior
```

Test matrix:
1. Cache hit — value returned from cache
2. Cache miss — value computed and cached
3. Cache invalidation — entry removed on update
4. Cache TTL — entry expires after TTL
5. Cache stampede — single computation, others wait
6. Cache coherence — multi-instance cache consistency
7. Cache eviction — LRU eviction when full
8. Cache size limit — memory bound enforced
9. Cache warmup — pre-populate on startup
10. Cache fallback — cache down, compute directly

### Message Queue/Pub-Sub Tests

```rust
#[test]
fn pubsub_{scenario}()
// For EVERY pub/sub behavior
```

Test matrix:
1. Message publish — message delivered to Redis
2. Message subscribe — subscriber receives message
3. At-least-once delivery — message not lost on failure
4. Message ordering — FIFO per channel
5. Message deduplication — duplicate message ID detected
6. Subscriber lag — slow subscriber doesn't block publisher
7. Channel creation — new channel auto-created
8. Channel deletion — messages cleaned up
9. Pattern subscribe — wildcard subscription works
10. Pub/Sub under load — no message loss at high throughput

### Upgrade/Migration Tests

```rust
#[test]
fn upgrade_{scenario}()
// For EVERY upgrade path
```

Test matrix:
1. Config migration — v1 config loaded by v2 runtime
2. Config migration — v2 config rejected by v1 runtime (if applicable)
3. Database migration — schema upgrade applied
4. Database migration — rollback on failure
5. Zero-downtime deploy — blue/green switch
6. Rolling upgrade — mixed-version cluster works
7. Binary upgrade — hot-swap binary without restart (if applicable)
8. PHP driver upgrade — new driver on old runtime
9. PHP driver downgrade — old driver on new runtime
10. Data migration — existing data migrated correctly

### Compliance/Regulatory Tests

```rust
#[test]
fn compliance_{requirement}_{scenario}()
// For EVERY compliance requirement
```

Test matrix:
1. Data residency — data stays in configured region
2. Audit logging — every action logged with timestamp, actor, action
3. Audit log integrity — logs tamper-evident
4. PII handling — PII redacted in logs
5. PII handling — PII encrypted at rest (if required)
6. Access control — unauthorized access blocked
7. Rate limiting — abuse prevention (DDoS protection)
8. Session security — session fixation prevention
9. Session security — session timeout enforced
10. Data retention — data deleted after retention period
11. GDPR — data export on request (if applicable)
12. GDPR — data deletion on request (if applicable)

---

## Quality Gate (Exhaustive)

Before finishing, EVERY checkbox must be checked:

- [ ] ALL happy path variants tested (every enum variant, every type combo)
- [ ] ALL error variants tested (every error arm)
- [ ] ALL edge cases from input type matrix tested
- [ ] ALL boundary values tested (at, below, above)
- [ ] ALL injection vectors tested (if external input)
- [ ] ALL encoding attacks tested (URL, base64, unicode, double-encode)
- [ ] ALL race conditions tested (if concurrent)
- [ ] ALL deadlock permutations tested (if multiple locks)
- [ ] ALL channel scenarios tested (if using channels)
- [ ] ALL resource leak scenarios tested (if managing resources)
- [ ] ALL throughput levels tested (if request handling)
- [ ] ALL spike loads tested (if exposed to traffic)
- [ ] Endurance test passed — no memory growth over 30+ minutes (if long-running)
- [ ] Thundering herd handled — no cascade failure (if shared resources)
- [ ] Backpressure enforced — rejected not dropped (if bounded queues)
- [ ] Degradation graceful — partial failure doesn't cascade (if dependencies)
- [ ] ALL serialization roundtrip tested (if serializer/deserializer)
- [ ] ALL config override precedence tested (if configuration)
- [ ] ALL error chains preserve context (if multi-layer errors)
- [ ] ALL state restoration tested (if persisted state)
- [ ] ALL idempotent operations verified (if retryable operations)
- [ ] ALL cross-boundary inputs validated (if IPC/FFI/WASM/network)
- [ ] ALL protocol violations tested (if protocol implementation)
- [ ] ALL lifecycle edge cases tested (partial init, clone+drop, Arc/weak)
- [ ] ALL properties/invariants hold under any operation sequence (if property-based)
- [ ] ALL graceful degradation paths tested (if external dependencies)
- [ ] ALL platform-specific behaviors tested (if cross-platform)
- [ ] ALL PHP engine types tested (FFI/WASM/child/embed) (if multi-engine)
- [ ] ALL worker lifecycle events tested (if worker pool)
- [ ] ALL request flow paths tested end-to-end (if HTTP server)
- [ ] ALL PHP state isolation verified between requests (if PHP runtime)
- [ ] ALL IPC protocol behaviors tested (if Rust↔PHP IPC)
- [ ] ALL VFS path traversal attempts blocked (if virtual file system)
- [ ] ALL tenant isolation boundaries verified (if multi-tenant)
- [ ] ALL hot-reload scenarios tested (if config/file watching)
- [ ] ALL plugin hooks execute correctly (if plugin system)
- [ ] ALL Laravel/Octane compatibility verified (if Laravel runtime)
- [ ] ALL security mechanisms enforced (Landlock, Seccomp, circuit breaker) (if hardening)
- [ ] ALL deployment scenarios verified (if containerized)
- [ ] ALL PHP driver package behaviors tested (if composer package)
- [ ] ALL observability signals accurate (traces, metrics, logs) (if observable)
- [ ] ALL benchmark baselines established and regression checked (if performance-critical)
- [ ] ALL fuzzing targets executed with coverage (if external input)
- [ ] ALL long-running stability tests passed (24hr+ no degradation) (if server)
- [ ] ALL chaos failure scenarios handled gracefully (if distributed)
- [ ] ALL memory profiling tests pass — no leaks, stable fragmentation (if long-running)
- [ ] ALL WebSocket behaviors tested (if realtime support)
- [ ] ALL TLS/ACME renewal scenarios tested (if HTTPS/ACME)
- [ ] ALL QUIC/HTTP3 behaviors tested (if QUIC enabled)
- [ ] ALL static file serving behaviors tested (if static file server)
- [ ] ALL external service integration behaviors tested (if DB/cache/queue)
- [ ] ALL upgrade/migration paths verified (if versioned system)
- [ ] ALL compliance requirements met (if regulated industry)
- [ ] Recovery clean — returns to baseline after stress (if stateful)
- [ ] ALL panic cleanup tested (drop after panic)
- [ ] ALL arithmetic overflow scenarios tested
- [ ] ALL off-by-one boundaries tested
- [ ] ALL iterator exhaustion tested
- [ ] ALL non-deterministic behavior tested (HashMap order, RNG)
- [ ] ALL valid state transitions tested (if state machine)
- [ ] ALL invalid state transitions rejected (compile-time or runtime)
- [ ] ALL business rules tested (boundary, conflict, empty, max)
- [ ] ALL invariants maintained after operations and errors
- [ ] ALL workflow paths tested (happy, missing step, wrong order, cancel)
- [ ] ALL parallel workflows isolated (no interference)
- [ ] ALL decision table combinations tested (if boolean conditions)
- [ ] ALL guard clauses violated and met (if preconditions)
- [ ] ALL combinatorial pairs tested (if multiple inputs)
- [ ] ALL time-dependent boundaries tested (if time-based logic)
- [ ] ALL event orderings tested (if event-driven)
- [ ] ALL fallback paths triggered (if fallback logic)
- [ ] ALL rate limit boundaries tested (if rate limiting)
- [ ] Unicode normalization (NFC/NFD/NFKC/NFKD) tested
- [ ] RTL/LTR override tested
- [ ] Homoglyph attacks tested
- [ ] Zero-width characters tested
- [ ] Null bytes tested
- [ ] Control characters tested
- [ ] Max length + 1 tested
- [ ] Empty input tested
- [ ] Whitespace-only input tested
- [ ] Timeout behavior tested (if async)
- [ ] Cancellation behavior tested (if async)
- [ ] Drop behavior after partial initialization tested
- [ ] Send/Sync bounds tested (compile-time verification)
- [ ] No `.unwrap()` in test assertions (use `?` or `expect`)
- [ ] Test names follow strict naming convention
- [ ] Every test has Arrange-Act-Assert structure

### Cybersecurity-Specific Quality Gate (NEW)

- [ ] ALL binary protocol decode paths handle malformed input without panic
- [ ] ALL SQL allowlist filters tested against 15+ bypass techniques
- [ ] ALL env variables tested with malicious values (fail-safe default)
- [ ] ALL path inputs tested for traversal (.., symlink, unicode, encoding)
- [ ] ALL cross-process communication tested for MITM/injection
- [ ] ALL PHP-Rust boundary tested for type confusion
- [ ] ALL integer-to-length conversions checked for overflow before allocation
- [ ] ALL opcodes/enums tested for unknown value handling (no panics)
- [ ] ALL feature flags tested with invalid env values (fail-closed)
- [ ] ALL zero-copy/mmap security boundaries validated (future v2 frame)
- [ ] ALL PDO/SQL injection surfaces tested for both Rust and PHP parity
- [ ] ALL stdio transport tested for partial write / pipe closure
- [ ] ALL worker pool tested for idle queue corruption under concurrency
- [ ] ALL PHP child process tested for SIGKILL / zombie prevention
- [ ] ALL shared SQLite connection tested for concurrent query safety

---

## Related Rust Skills

When generating exhaustive tests, trace to these skills for design decisions:

| Test Aspect | When | See Skill | Key Insight |
|---|---|---|---|
| Ownership patterns | Testing move, borrow, lifetime in test setup | m01-ownership | Test owned, borrowed, and reference variants separately |
| Smart pointers | Testing Arc/Rc/Weak in concurrent context | m02-resource | Arc for multi-thread tests, Rc for single-thread |
| Mutability | Testing Cell/RefCell/Mutex/RwLock | m03-mutability | Test interior mutability under concurrent access |
| Generics/traits | Testing generic functions, trait bounds, dyn | m04-zero-cost | Test with concrete type substitutions and trait objects |
| Type-driven | Testing newtype, type state, builder, ZST | m05-type-driven | Invalid states should fail at compile-time AND runtime |
| Error handling | Testing Result/Option, unwrap vs ? vs expect | m06-error-handling | Test every error variant, context propagation, panic cases |
| Concurrency | Testing async, channels, Send/Sync, deadlock | m07-concurrency | Multi-thread flavor, lock ordering, MutexGuard across await |
| Resource lifecycle | Testing Drop, RAII, connection pools, OnceLock | m12-lifecycle | Cleanup on panic, partial initialization drop, RAII invariant |
| Domain model | Testing entity, value object, aggregate, repository | m09-domain | State transitions, aggregate invariants, boundary enforcement |
| Performance | Testing benchmark, allocation, cache, SIMD | m10-performance | Profile first, criterion, P50/P99 latency, allocation counting |
| Error strategy | Testing retry, circuit breaker, graceful degradation | m13-domain-error | Transient vs permanent, backoff, fallback behavior |
| Anti-patterns | Testing clone everywhere, unwrap in prod, unsafe | m15-anti-pattern | Quality gate: no clone without justification, SAFETY comments |
| Code style | Test naming, organization, iterator patterns | coding-guidelines | snake_case test names, arrange-act-assert, no index loops |
| Ecosystem | Testing crate features, dependency combinations | m11-ecosystem | Feature flag matrix, optional dependency testing |
| Mental models | Understanding why tests fail | m14-mental-model | Ownership visualization, borrow chain analysis |
| Domain-specific | Web, fintech, IoT, ML | domain-* | Domain-specific security, precision, protocol testing |

---

## Additional Resources

- For detailed pattern implementations: [reference.md](reference.md)
- For security-specific patterns: [security-exhaustive.md](security-exhaustive.md)
- For concurrency patterns: [concurrency-exhaustive.md](concurrency-exhaustive.md)
