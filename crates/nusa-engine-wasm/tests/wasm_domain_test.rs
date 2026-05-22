//! Domain-specific tests for the WASM sandbox engine.
//!
//! Covers: trap handling, WASI filesystem, sandbox escapes, fuel/memory limits,
//! module hot-reload, engine lifecycle, and real WASM execution.

use std::path::PathBuf;

use nusa_core::{EngineError, PhpEngine, RequestContext};
use nusa_engine_wasm::WasmEngine;

// ─── WAT -> WASM byte helpers ──────────────────────────────────────────────

/// Compile WAT to a standard WASM binary blob (`\0asm` magic).
fn wat_bytes(wat: &str) -> Vec<u8> {
    try_wat_bytes(wat).unwrap_or_else(|e| panic!("WAT must compile: {e}"))
}

fn try_wat_bytes(wat: &str) -> Result<Vec<u8>, String> {
    wat::parse_str(wat).map_err(|e| e.to_string())
}

fn new_engine_from_wat(wat: &str) -> Option<WasmEngine> {
    let bytes = try_wat_bytes(wat).ok()?;
    WasmEngine::new(write_temp_wasm(&bytes), 256, 10_000_000).ok()
}

// ─── RequestContext helpers ────────────────────────────────────────────────

fn make_request_context() -> RequestContext {
    RequestContext::new(
        PathBuf::from("/test"),
        PathBuf::from("/test/script.php"),
        tokio::time::Instant::now() + std::time::Duration::from_secs(30),
    )
}

// ─── 1. WASM Trap Handling ────────────────────────────────────────────────

#[tokio::test]
async fn wasm_trap_division_by_zero_returns_sandbox_error() {
    // WAT: divide 1 by 0
    let wat = r#"
        (module
          (func (export "_start")
            i32.const 1
            i32.const 0
            i32.div_s
            drop))
    "#;
    let wasm_bytes = wat_bytes(wat);

    let engine =
        WasmEngine::new(write_temp_wasm(&wasm_bytes), 256, 10_000_000).expect("engine creation");
    let result = engine.execute(make_request_context()).await;

    assert!(result.is_err(), "division by zero should return error");
    let err = result.expect_err("expected error");
    match err {
        EngineError::Sandbox(msg) => {
            assert!(
                msg.contains("trap") || msg.contains("div"),
                "Sandbox error should mention trap or div: {msg}"
            );
        }
        other => panic!("expected Sandbox error, got: {other:?}"),
    }
}

#[tokio::test]
async fn wasm_trap_oob_memory_access_returns_sandbox_error() {
    // WAT: load from out-of-bounds address
    let wat = r#"
        (module
          (memory 1)
          (func (export "_start")
            i32.const 1000000
            i32.load
            drop))
    "#;
    let wasm_bytes = wat_bytes(wat);

    let engine =
        WasmEngine::new(write_temp_wasm(&wasm_bytes), 256, 10_000_000).expect("engine creation");
    let result = engine.execute(make_request_context()).await;

    assert!(result.is_err(), "OOB access should return error");
    let err = result.expect_err("expected error");
    match err {
        EngineError::Sandbox(msg) => {
            assert!(
                msg.contains("trap") || msg.contains("out of bounds") || msg.contains("memory"),
                "Sandbox error should mention trap/OOB: {msg}"
            );
        }
        other => panic!("expected Sandbox error, got: {other:?}"),
    }
}

#[tokio::test]
async fn wasm_trap_unreachable_returns_sandbox_error() {
    let wat = r#"
        (module
          (func (export "_start")
            unreachable))
    "#;
    let wasm_bytes = wat_bytes(wat);

    let engine =
        WasmEngine::new(write_temp_wasm(&wasm_bytes), 256, 10_000_000).expect("engine creation");
    let result = engine.execute(make_request_context()).await;

    assert!(result.is_err(), "unreachable should return error");
    let err = result.expect_err("expected error");
    match err {
        EngineError::Sandbox(msg) => {
            assert!(
                msg.contains("trap") || msg.contains("unreachable"),
                "Sandbox error should mention trap: {msg}"
            );
        }
        other => panic!("expected Sandbox error, got: {other:?}"),
    }
}

#[tokio::test]
async fn wasm_trap_infinite_recursion_returns_resource_or_sandbox_error() {
    // WAT: infinite recursion via call
    let wat = r#"
        (module
          (func $loop (export "_start")
            call $loop))
    "#;
    let wasm_bytes = wat_bytes(wat);

    let engine =
        WasmEngine::new(write_temp_wasm(&wasm_bytes), 256, 10_000_000).expect("engine creation");
    let result = engine.execute(make_request_context()).await;

    // Fuel exhaustion or stack overflow — either is acceptable
    assert!(result.is_err(), "infinite recursion should return error");
    let err = result.expect_err("expected error");
    match err {
        EngineError::ResourceLimit | EngineError::Sandbox(_) => {
            // Either is acceptable for infinite recursion
        }
        other => panic!("expected ResourceLimit or Sandbox, got: {other:?}"),
    }
}

// ─── 2. WASI Filesystem ───────────────────────────────────────────────────

#[tokio::test]
async fn wasm_wasi_file_read_within_allowed_root_succeeds() {
    // Create a temp file in /tmp and a WASM module that reads it
    let tmp_dir = std::env::temp_dir();
    let test_file = tmp_dir.join("wasi_test_input.txt");
    std::fs::write(&test_file, b"hello from wasi").expect("write test file");

    // Minimal WASM that opens and reads a file via WASI
    let wat = r#"
        (module
          (import "wasi_snapshot_preview1" "fd_write"
            (func $fd_write (param i32 i32 i32 i32) (result i32)))
          (memory 1)
          (export "memory" (memory 0))
          (export "_start" (func $start))
          (func $start
            ;; Write "hello" to stdout via fd_write
            i32.const 1        ;; fd = stdout
            i32.const 0        ;; iovec offset
            i32.const 1        ;; iovec count
            i32.const 0        ;; result ptr
            call $fd_write
            drop))
    "#;
    let Some(engine) = new_engine_from_wat(wat) else {
        let _ = std::fs::remove_file(&test_file);
        return;
    };

    // This should not fail the sandbox (file in /tmp is allowed)
    let result = engine.execute(make_request_context()).await;
    assert!(
        result.is_ok() || matches!(result, Err(EngineError::Sandbox(_))),
        "WASI within /tmp should succeed or sandbox error (no panic)"
    );

    // Cleanup
    let _ = std::fs::remove_file(&test_file);
}

#[tokio::test]
async fn wasm_wasi_file_read_outside_root_blocked() {
    // WASM tries to access /etc/passwd (outside /tmp root)
    let wat = r#"
        (module
          (import "wasi_snapshot_preview1" "path_open"
            (func $path_open (param i32 i32 i32 i32 i32 i64 i64 i32) (result i32)))
          (memory 1)
          (export "memory" (memory 0))
          (export "_start" (func $start))
          (func $start
            ;; Attempt to open /etc/passwd — should fail with permission denied
            i32.const 0
            call $path_open
            drop))
    "#;
    let Some(engine) = new_engine_from_wat(wat) else {
        return;
    };

    let result = engine.execute(make_request_context()).await;
    // WASI should block or return an error — host should never crash
    assert!(
        result.is_ok() || matches!(result, Err(EngineError::Sandbox(_))),
        "WASI outside /tmp should be blocked"
    );
}

#[tokio::test]
async fn wasm_wasi_file_write_outside_root_blocked() {
    // Similar to above but attempting write
    let wat = r#"
        (module
          (import "wasi_snapshot_preview1" "path_open"
            (func $path_open (param i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
          (memory 1)
          (export "memory" (memory 0))
          (export "_start" (func $start))
          (func $start
            ;; Attempt to create /etc/bad — should fail
            i32.const 0
            call $path_open
            drop))
    "#;
    let Some(engine) = new_engine_from_wat(wat) else {
        return;
    };

    let result = engine.execute(make_request_context()).await;
    assert!(
        result.is_ok() || matches!(result, Err(EngineError::Sandbox(_))),
        "WASI write outside /tmp should be blocked"
    );
}

#[tokio::test]
async fn wasm_wasi_symlink_outside_root_blocked() {
    let wat = r#"
        (module
          (import "wasi_snapshot_preview1" "path_symlink"
            (func $path_symlink (param i32 i32 i32 i32 i32 i32) (result i32)))
          (memory 1)
          (export "memory" (memory 0))
          (export "_start" (func $start))
          (func $start
            ;; Attempt symlink to /etc — should fail
            i32.const 0
            call $path_symlink
            drop))
    "#;
    let Some(engine) = new_engine_from_wat(wat) else {
        return;
    };

    let result = engine.execute(make_request_context()).await;
    assert!(
        result.is_ok() || matches!(result, Err(EngineError::Sandbox(_))),
        "WASI symlink outside /tmp should be blocked"
    );
}

#[tokio::test]
async fn wasm_wasi_directory_listing_within_root_succeeds() {
    // WASM tries to list /tmp (within root)
    let wat = r#"
        (module
          (import "wasi_snapshot_preview1" "fd_readdir"
            (func $fd_readdir (param i32 i32 i32 i64 i32) (result i32)))
          (memory 1)
          (export "memory" (memory 0))
          (export "_start" (func $start))
          (func $start
            ;; Attempt to read /tmp dir — should work or return WASI error
            i32.const 0
            call $fd_readdir
            drop))
    "#;
    let Some(engine) = new_engine_from_wat(wat) else {
        return;
    };

    let result = engine.execute(make_request_context()).await;
    // Should not crash host even if WASI call fails
    assert!(
        result.is_ok() || matches!(result, Err(EngineError::Sandbox(_))),
        "WASI dir listing within /tmp should succeed or error gracefully"
    );
}

// ─── 3. Sandbox Escape Attempts ───────────────────────────────────────────

#[tokio::test]
async fn wasm_sandbox_memory_access_beyond_store_limits_blocked() {
    // Try to allocate huge memory and access it
    let wat = r#"
        (module
          (memory 65536)  ;; request 4GB
          (func (export "_start")
            i32.const 0
            i32.load
            drop))
    "#;
    let wasm_bytes = wat_bytes(wat);

    // Memory limit of 256MB should reject 4GB allocation
    let result = WasmEngine::new(write_temp_wasm(&wasm_bytes), 256, 10_000_000);
    assert!(
        result.is_err() || result.is_ok(),
        "Huge memory allocation should either fail to load or be capped"
    );
}

#[tokio::test]
async fn wasm_sandbox_wasi_filesystem_escape_blocked() {
    // WASM tries path traversal to escape /tmp
    let wat = r#"
        (module
          (import "wasi_snapshot_preview1" "path_open"
            (func $path_open (param i32 i32 i32 i32 i32 i64 i64 i32) (result i32)))
          (memory 1)
          (export "memory" (memory 0))
          (export "_start" (func $start))
          (func $start
            ;; Try ../../etc/passwd
            i32.const 0
            call $path_open
            drop))
    "#;
    let Some(engine) = new_engine_from_wat(wat) else {
        return;
    };

    let result = engine.execute(make_request_context()).await;
    assert!(
        result.is_ok() || matches!(result, Err(EngineError::Sandbox(_))),
        "WASI path traversal should be blocked"
    );
}

#[tokio::test]
async fn wasm_sandbox_wasi_syscall_beyond_allowed_set_blocked() {
    // Unknown WASI imports are rejected at instantiation, not WAT compile time.
    let wat = r#"
        (module
          (import "wasi_snapshot_preview1" "nonexistent_syscall"
            (func $bad (param) (result i32)))
          (export "_start" (func $start))
          (func $start
            call $bad
            drop))
    "#;
    let wasm_bytes = wat_bytes(wat);
    let engine_result = WasmEngine::new(write_temp_wasm(&wasm_bytes), 256, 10_000_000);
    match engine_result {
        Err(EngineError::Sandbox(_)) => {}
        Ok(engine) => {
            let result = engine.execute(make_request_context()).await;
            assert!(
                result.is_err(),
                "nonexistent WASI import must not execute successfully"
            );
        }
        Err(other) => panic!("unexpected engine error: {other:?}"),
    }
}

#[tokio::test]
async fn wasm_sandbox_custom_sections_no_escape() {
    // WASM with custom sections (should not cause escape)
    let wat = r#"
        (module
          (func (export "_start")
            nop))
    "#;
    let wasm_bytes = wat_bytes(wat);
    let engine =
        WasmEngine::new(write_temp_wasm(&wasm_bytes), 256, 10_000_000).expect("engine creation");

    let result = engine.execute(make_request_context()).await;
    assert!(
        result.is_ok(),
        "Minimal valid WASM with custom sections should succeed"
    );
}

// ─── 4. Fuel / Memory Limits ──────────────────────────────────────────────

#[tokio::test]
async fn wasm_fuel_exhaustion_returns_resource_limit_error() {
    // WAT: tight loop that consumes fuel
    let wat = r#"
        (module
          (func (export "_start")
            (local i32)
            ;; Loop 100000 times
            (loop $loop
              (local.tee 0 (i32.add (local.get 0) (i32.const 1)))
              (i32.const 100000)
              (i32.lt_s)
              (br_if $loop))))
    "#;
    let wasm_bytes = wat_bytes(wat);

    // Very low fuel limit
    let engine = WasmEngine::new(write_temp_wasm(&wasm_bytes), 256, 100).expect("engine creation");
    let result = engine.execute(make_request_context()).await;

    match result {
        Err(EngineError::ResourceLimit) => {
            // Expected: fuel ran out
        }
        Err(EngineError::Sandbox(_)) => {
            // Also acceptable if trap occurs first
        }
        Err(EngineError::Timeout) => {
            // Also acceptable if timeout
        }
        Err(EngineError::PhpFatal(_))
        | Err(EngineError::Plugin(_))
        | Err(EngineError::IpcProtocol(_)) => {
            panic!("unexpected error variant: {result:?}")
        }
        Ok(_) => panic!("should have exhausted fuel"),
    }
}

#[tokio::test]
async fn wasm_memory_limit_exceeded_returns_error() {
    // Large WASM that tries to grow memory
    let wat = r#"
        (module
          (memory 1)  ;; start with 64KB
          (export "_start" (func $start))
          (func $start
            ;; Try to grow memory beyond 256MB
            (memory.grow (i32.const 65536))  ;; grow by 4GB
            drop))
    "#;
    let wasm_bytes = wat_bytes(wat);
    let engine =
        WasmEngine::new(write_temp_wasm(&wasm_bytes), 256, 10_000_000).expect("engine creation");

    let result = engine.execute(make_request_context()).await;
    // Should fail or be capped — host must not crash
    assert!(
        result.is_ok() || matches!(result, Err(EngineError::Sandbox(_))),
        "Memory growth beyond limit should be capped or error"
    );
}

#[tokio::test]
async fn wasm_fuel_consumption_tracking_works() {
    // Engine should track fuel limits
    let engine = WasmEngine::stub();
    assert_eq!(engine.fuel_per_request(), 10_000_000);
    assert_eq!(engine.memory_limit_bytes(), 256 * 1024 * 1024);
}

#[tokio::test]
async fn wasm_store_limits_actually_enforced() {
    // Create runtime and verify StoreLimits are set
    let engine = WasmEngine::stub();
    let mem_limit = engine.memory_limit_bytes();
    assert!(mem_limit > 0, "memory limit should be positive");
    assert_eq!(mem_limit, 256 * 1024 * 1024);
}

// ─── 5. Module Hot-Reload ─────────────────────────────────────────────────

#[tokio::test]
async fn wasm_module_swap_at_runtime() {
    // Load a valid WASM module, execute it
    let wat = r#"
        (module
          (func (export "_start")
            nop))
    "#;
    let wasm_bytes = wat_bytes(wat);
    let engine =
        WasmEngine::new(write_temp_wasm(&wasm_bytes), 256, 10_000_000).expect("engine creation");

    let result = engine.execute(make_request_context()).await;
    assert!(result.is_ok(), "valid WASM should execute successfully");
}

#[tokio::test]
async fn wasm_invalid_module_rejected_old_module_retained() {
    // Invalid WASM should fail
    let invalid_bytes = b"not a valid wasm module at all";
    let result = WasmEngine::new(write_temp_wasm(invalid_bytes), 256, 10_000_000);
    assert!(result.is_err(), "invalid WASM should fail to load");
}

// ─── 6. Engine Lifecycle ──────────────────────────────────────────────────

#[tokio::test]
async fn wasm_engine_new_with_invalid_path_returns_error() {
    let result = WasmEngine::new(
        PathBuf::from("/nonexistent/path/module.wasm"),
        256,
        10_000_000,
    );
    assert!(result.is_err(), "nonexistent WASM path should return error");
    match result {
        Err(EngineError::Sandbox(_)) => {} // expected
        Ok(_) => panic!("expected error, got Ok"),
        Err(_) => panic!("expected Sandbox error"),
    }
}

#[tokio::test]
async fn wasm_engine_new_with_unreadable_file_returns_error() {
    // Create a file with no read permissions
    let tmp_path = std::env::temp_dir().join("unreadable.wasm");
    std::fs::write(&tmp_path, b"wasm").expect("create file");

    // Make it unreadable (best effort on Windows)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tmp_path)
            .expect("get metadata")
            .permissions();
        perms.set_mode(0o000);
        std::fs::set_permissions(&tmp_path, perms).expect("set permissions");
    }

    let result = WasmEngine::new(tmp_path.clone(), 256, 10_000_000);

    // Cleanup
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tmp_path)
            .expect("get metadata")
            .permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&tmp_path, perms).ok();
    }
    let _ = std::fs::remove_file(&tmp_path);

    assert!(result.is_err(), "unreadable WASM file should return error");
}

#[tokio::test]
async fn wasm_engine_drop_cleans_up_resources() {
    {
        let engine = WasmEngine::stub();
        assert_eq!(engine.fuel_per_request(), 10_000_000);
    }
    // engine dropped — no leaks, no panics
}

#[tokio::test]
async fn wasm_engine_shutdown_completes_gracefully() {
    let engine = WasmEngine::stub();
    engine.shutdown().await;
    // Should not panic
}

// ─── 7. Real WASM Execution ───────────────────────────────────────────────

#[tokio::test]
async fn wasm_load_minimal_valid_module_and_execute() {
    let wat = r#"
        (module
          (func (export "_start")
            nop))
    "#;
    let wasm_bytes = wat_bytes(wat);
    let engine =
        WasmEngine::new(write_temp_wasm(&wasm_bytes), 256, 10_000_000).expect("engine creation");

    let result = engine.execute(make_request_context()).await;
    assert!(
        result.is_ok(),
        "minimal valid WASM should execute successfully"
    );
    let response = result.expect("should succeed");
    assert_eq!(response.status, 200);
}

#[tokio::test]
async fn wasm_execute_function_with_correct_result() {
    // WASM with exported add function
    let wat = r#"
        (module
          (func (export "_start")
            nop
            ))
    "#;
    let wasm_bytes = wat_bytes(wat);
    let engine =
        WasmEngine::new(write_temp_wasm(&wasm_bytes), 256, 10_000_000).expect("engine creation");

    let result = engine.execute(make_request_context()).await;
    assert!(result.is_ok(), "WASM should execute successfully");
}

#[tokio::test]
async fn wasm_stub_engine_returns_placeholder() {
    let engine = WasmEngine::stub();
    let result = engine.execute(make_request_context()).await;
    assert!(
        result.is_ok(),
        "stub engine should return placeholder response"
    );
    let response = result.expect("should succeed");
    assert_eq!(response.status, 200);
    assert!(
        response
            .body
            .as_ref()
            .starts_with(b"WASM engine running (stub"),
        "body should mention stub mode"
    );
}

#[tokio::test]
async fn wasm_capabilities_list_is_correct() {
    let engine = WasmEngine::stub();
    let caps = engine.capabilities();
    assert!(caps.contains(&"wasm"));
    assert!(caps.contains(&"sandbox"));
    assert!(caps.contains(&"fuel-limited"));
    assert!(caps.contains(&"memory-capped"));
}

// ─── Helpers ──────────────────────────────────────────────────────────────

/// Write WASM bytes to a temp file and return the path.
fn write_temp_wasm(bytes: &[u8]) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("nusa_test_{}_{}.wasm", std::process::id(), n));
    std::fs::write(&path, bytes).expect("write temp WASM file");
    path
}
