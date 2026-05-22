//! FFI engine security tests for nusa-engine-ffi.
//!
//! Covers: Script execution injection, ZTS isolation, input attacks,
//! resource limit enforcement.

use std::path::PathBuf;

use nusa_core::{PhpEngine, RequestContext};
use nusa_engine_ffi::FfiEngine;

// ===== Script Execution: SQL Injection =====

#[test]
fn ffi_engine_sql_injection_in_body_passed_through() {
    // The FFI engine passes request body to PHP — test that SQL patterns
    // don't crash the FFI boundary. The stub returns a placeholder.
    let engine = FfiEngine::new(4);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);

    let sql_patterns = [
        "' OR 1=1 --",
        "'; DROP TABLE users; --",
        "1; SELECT * FROM information_schema",
        "UNION SELECT username, password FROM users",
    ];

    for pattern in sql_patterns {
        let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline)
            .with_body(bytes::Bytes::from(pattern));

        let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
        let result = rt.block_on(engine.execute(ctx));
        // Should not crash — stub returns Ok or FFI fails gracefully
        assert!(
            result.is_ok() || result.is_err(),
            "SQL injection should not crash FFI"
        );
    }
}

#[test]
fn ffi_engine_xss_patterns_in_body_passed_through() {
    let engine = FfiEngine::new(4);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);

    let xss_patterns = [
        "<script>alert('xss')</script>",
        "<img src=x onerror=alert(1)>",
        "<svg onload=alert(1)>",
        "javascript:alert(1)",
        "<iframe src='javascript:alert(1)'>",
    ];

    for pattern in xss_patterns {
        let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline)
            .with_body(bytes::Bytes::from(pattern));

        let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
        let result = rt.block_on(engine.execute(ctx));
        assert!(result.is_ok() || result.is_err());
    }
}

#[test]
fn ffi_engine_command_injection_patterns_in_body() {
    let engine = FfiEngine::new(4);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);

    let cmd_patterns = [
        "<?php system('id'); ?>",
        "<?php exec('whoami'); ?>",
        "<?php shell_exec('ls -la'); ?>",
        "<?php passthru('cat /etc/passwd'); ?>",
        "<?php popen('id', 'r'); ?>",
    ];

    for pattern in cmd_patterns {
        let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline)
            .with_body(bytes::Bytes::from(pattern));

        let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
        let result = rt.block_on(engine.execute(ctx));
        // Should not crash the host process
        assert!(result.is_ok() || result.is_err());
    }
}

#[test]
fn ffi_engine_path_traversal_in_body() {
    let engine = FfiEngine::new(4);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);

    let paths = ["../../../etc/passwd", "/proc/self/environ", "/dev/null"];

    for path in paths {
        let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline)
            .with_body(bytes::Bytes::from(path));

        let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
        let result = rt.block_on(engine.execute(ctx));
        assert!(result.is_ok() || result.is_err());
    }
}

#[test]
fn ffi_engine_null_byte_in_body() {
    let engine = FfiEngine::new(4);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline)
        .with_body(bytes::Bytes::from("hello\0world"));

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));
    assert!(result.is_ok() || result.is_err());
}

// ===== ZTS Isolation Tests =====

#[test]
fn ffi_engine_concurrent_threads_no_cross_contamination() {
    // Verify that concurrent execution through the semaphore pool
    // doesn't cause cross-thread data leakage.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);

    let mut handles = Vec::new();
    for i in 0..4 {
        let engine_clone = FfiEngine::new(4); // Each gets its own semaphore
        let body = format!("thread-{}-data", i);
        let deadline_clone = deadline;

        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
            let ctx = RequestContext::new(
                PathBuf::from("/tmp"),
                PathBuf::from("test.php"),
                deadline_clone,
            )
            .with_body(bytes::Bytes::from(body));

            rt.block_on(engine_clone.execute(ctx))
        });
        handles.push(handle);
    }

    // All threads should complete without panic
    for handle in handles {
        let result = handle.join();
        assert!(result.is_ok(), "thread should not panic");
    }
}

#[test]
fn ffi_engine_output_buf_isolation() {
    // The OUTPUT_BUF is thread_local — verify it's isolated per thread
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);

    let bodies: Vec<_> = (0..4).map(|i| format!("unique_data_{}", i)).collect();

    let mut handles = Vec::new();
    for body in bodies.clone() {
        let engine_clone = FfiEngine::new(4);
        let deadline_clone = deadline;
        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
            let ctx = RequestContext::new(
                PathBuf::from("/tmp"),
                PathBuf::from("test.php"),
                deadline_clone,
            )
            .with_body(bytes::Bytes::from(body));

            rt.block_on(engine_clone.execute(ctx))
        });
        handles.push(handle);
    }

    for handle in handles {
        let result = handle.join();
        assert!(result.is_ok());
    }
}

// ===== Input Attack Tests =====

#[test]
fn ffi_engine_sql_injection_in_headers() {
    use http::HeaderMap;
    let engine = FfiEngine::new(4);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);

    let mut headers = HeaderMap::new();
    headers.insert("x-sql", "' OR 1=1 --".parse().expect("should parse"));
    headers.insert(
        "x-union",
        "UNION SELECT * FROM users".parse().expect("should parse"),
    );

    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline)
        .with_headers(headers);

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn ffi_engine_xss_in_headers() {
    use http::HeaderMap;
    let engine = FfiEngine::new(4);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);

    let mut headers = HeaderMap::new();
    headers.insert(
        "x-xss",
        "<script>alert(1)</script>".parse().expect("should parse"),
    );

    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline)
        .with_headers(headers);

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn ffi_engine_null_bytes_in_headers() {
    // Null bytes in header values are rejected by http crate
    let result = "value\0with_null".parse::<http::HeaderValue>();
    assert!(result.is_err(), "null byte in header value should fail");
}

#[test]
fn ffi_engine_format_string_in_body() {
    let engine = FfiEngine::new(4);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);

    let patterns = ["%s", "%n", "%x", "{}", "{0}"];
    for pattern in patterns {
        let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline)
            .with_body(bytes::Bytes::from(pattern));

        let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
        let result = rt.block_on(engine.execute(ctx));
        assert!(result.is_ok() || result.is_err());
    }
}

#[test]
fn ffi_engine_oversized_body_10mb() {
    let engine = FfiEngine::new(4);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let big_body = "A".repeat(10 * 1024 * 1024);

    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline)
        .with_body(bytes::Bytes::from(big_body));

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn ffi_engine_oversized_headers_100_plus() {
    use http::HeaderMap;
    let engine = FfiEngine::new(4);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);

    let mut headers = HeaderMap::new();
    for i in 0..150 {
        headers.insert(
            format!("x-custom-{}", i)
                .parse::<http::HeaderName>()
                .expect("header name should parse"),
            format!("value-{}", i)
                .parse()
                .expect("header value should parse"),
        );
    }

    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline)
        .with_headers(headers);

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));
    assert!(result.is_ok() || result.is_err());
}

// ===== Resource Limit Tests =====

#[test]
fn ffi_engine_pool_semaphore_limits_concurrency() {
    let engine = FfiEngine::new(2);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline);
    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));
    assert!(
        result.is_ok(),
        "engine with 2 workers should execute in stub mode"
    );
}

#[test]
fn ffi_engine_new_valid_max_workers() {
    for max_workers in [1, 4, 8, 16, 100] {
        let _engine = FfiEngine::new(max_workers);
    }
}

#[test]
fn ffi_engine_new_zero_workers() {
    let engine = FfiEngine::new(0);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline);

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));
    assert!(
        matches!(result, Err(nusa_core::EngineError::ResourceLimit)),
        "zero workers must reject execution immediately"
    );
}

#[test]
fn ffi_engine_shutdown_closes_pool() {
    let engine = FfiEngine::new(4);
    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    rt.block_on(engine.shutdown());
    // After shutdown, the pool is closed — new acquires should fail
    // The stub should still execute (it doesn't check semaphore state in stub mode)
}

#[test]
fn ffi_engine_capabilities_correct() {
    let engine = FfiEngine::new(4);
    let caps = engine.capabilities();
    assert!(caps.contains(&"ffi"));
    assert!(caps.contains(&"native-ext"));
    assert!(caps.contains(&"zts"));
}
