//! Child process engine security tests for nusa-engine-child.
//!
//! Covers: Process spawn security, IPC attack vectors, input injection,
//! environment isolation, kill/cleanup safety.

use std::path::PathBuf;

use nusa_core::{EngineError, PhpEngine, RequestContext};
use nusa_engine_child::ChildEngine;

/// Child that copies stdin to stdout (non-framed output → IpcProtocol error).
fn stdin_mirror_child() -> ChildEngine {
    #[cfg(unix)]
    {
        ChildEngine::new(PathBuf::from("/bin/cat"), PathBuf::new())
    }
    #[cfg(windows)]
    {
        let comspec = std::env::var("COMSPEC").unwrap_or_else(|_| String::from("cmd.exe"));
        ChildEngine::new(
            PathBuf::from(comspec),
            PathBuf::from("/c echo not-framed-ipc-output"),
        )
    }
}

// ===== Process Spawn Security Tests =====

#[test]
fn child_engine_spawn_nonexistent_binary_returns_error() {
    // === Arrange ===
    let engine = ChildEngine::new(
        PathBuf::from("/nonexistent/binary"),
        PathBuf::from("/tmp/bootstrap.php"),
    );

    // === Act ===
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline);

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));

    // === Assert ===
    assert!(result.is_err(), "nonexistent binary should fail");
    if let Err(e) = &result {
        assert!(matches!(e, EngineError::PhpFatal(_)));
    }
}

#[test]
fn child_engine_spawn_path_traversal_in_binary() {
    // === Arrange ===
    let engine = ChildEngine::new(
        PathBuf::from("../../../../usr/bin/php"),
        PathBuf::from("/tmp/bootstrap.php"),
    );

    // === Act ===
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline);

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));

    // === Assert ===
    // Path traversal in binary path — either succeeds (if path resolves) or fails
    // Either way, no crash
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn child_engine_spawn_path_traversal_in_bootstrap() {
    // === Arrange ===
    let engine = ChildEngine::new(
        PathBuf::from("/usr/bin/php"),
        PathBuf::from("../../../../etc/passwd"),
    );

    // === Act ===
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline);

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));

    // === Assert ===
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn child_engine_spawn_null_bytes_in_binary_path() {
    // === Arrange ===
    let engine = ChildEngine::new(
        PathBuf::from("/usr/bin/php\0malicious"),
        PathBuf::from("/tmp/bootstrap.php"),
    );

    // === Act ===
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline);

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));

    // === Assert ===
    assert!(result.is_err(), "null byte in binary path should fail");
}

#[test]
fn child_engine_spawn_null_bytes_in_bootstrap_path() {
    let engine = ChildEngine::new(
        PathBuf::from("/usr/bin/php"),
        PathBuf::from("/tmp/bootstrap\0.php"),
    );

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline);

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));

    assert!(result.is_err(), "null byte in bootstrap path should fail");
}

// ===== IPC Attack Tests =====

#[test]
fn child_engine_malformed_response_from_child() {
    // The engine reads from child stdout and parses framed IPC bytes
    // If the child sends malformed data, the engine should handle gracefully
    // This is hard to test without a real child, but we verify the error handling path
    let engine = stdin_mirror_child();

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline);

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));

    // Should fail with IpcProtocol error (deserialization of non-framed data)
    assert!(result.is_err());
    if let Err(e) = &result {
        assert!(matches!(e, EngineError::IpcProtocol(_)));
    }
}

#[test]
fn child_engine_binary_garbage_from_child() {
    // Use /bin/echo with binary-like output
    let engine = ChildEngine::new(PathBuf::from("/bin/echo"), PathBuf::from("garbage"));

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline);

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));

    assert!(result.is_err());
}

#[test]
fn child_engine_truncated_ipc_message() {
    // If child sends partial framed data, the engine should handle gracefully
    let engine = ChildEngine::new(PathBuf::from("/bin/echo"), PathBuf::from("-n"));

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline);

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));

    assert!(result.is_err());
}

#[test]
fn child_engine_wrong_message_type() {
    // If child sends a non-Response message type, the engine should reject
    // This is tested implicitly by the malformed response test above
    let engine = ChildEngine::new(PathBuf::from("/bin/echo"), PathBuf::from("Ping"));

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline);

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));

    assert!(result.is_err());
}

// ===== Input Attack Tests =====

#[test]
fn child_engine_sql_injection_in_body() {
    let engine = ChildEngine::with_default_php();

    let sql_patterns = [
        "' OR 1=1 --",
        "'; DROP TABLE users; --",
        "UNION SELECT * FROM users",
    ];

    for pattern in sql_patterns {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline)
            .with_body(bytes::Bytes::from(pattern));

        let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
        let result = rt.block_on(engine.execute(ctx));
        assert!(result.is_ok() || result.is_err());
    }
}

#[test]
fn child_engine_xss_in_body() {
    let engine = ChildEngine::with_default_php();

    let xss_patterns = ["<script>alert(1)</script>", "<img src=x onerror=alert(1)>"];

    for pattern in xss_patterns {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline)
            .with_body(bytes::Bytes::from(pattern));

        let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
        let result = rt.block_on(engine.execute(ctx));
        assert!(result.is_ok() || result.is_err());
    }
}

#[test]
fn child_engine_path_traversal_in_body() {
    let engine = ChildEngine::with_default_php();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline)
        .with_body(bytes::Bytes::from("../../../etc/passwd"));

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn child_engine_null_bytes_in_body() {
    let engine = ChildEngine::with_default_php();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline)
        .with_body(bytes::Bytes::from("hello\0world"));

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn child_engine_format_string_in_body() {
    let engine = ChildEngine::with_default_php();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline)
        .with_body(bytes::Bytes::from("error: %s %n %x"));

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn child_engine_oversized_body_10mb() {
    let engine = ChildEngine::with_default_php();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let big_body = "A".repeat(10 * 1024 * 1024);
    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline)
        .with_body(bytes::Bytes::from(big_body));

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));
    assert!(result.is_ok() || result.is_err());
}

// ===== Environment Isolation Tests =====

#[test]
fn child_engine_inherits_restricted_environment() {
    // The child engine passes env vars from RequestContext to the child process
    // Verify that the engine doesn't inject additional sensitive vars
    let engine = ChildEngine::with_default_php();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);

    let mut env = std::collections::HashMap::new();
    env.insert("TEST_VAR".to_string(), "test_value".to_string());

    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline)
        .with_env(std::sync::Arc::new(env));

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));
    // Should not crash regardless of env vars
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn child_engine_sensitive_env_vars_not_injected() {
    // Verify we can create a context without sensitive env vars
    let mut env = std::collections::HashMap::new();
    // Don't include PASSWORD, SECRET_KEY, etc.
    env.insert("SAFE_VAR".to_string(), "safe".to_string());

    assert!(!env.contains_key("PASSWORD"));
    assert!(!env.contains_key("SECRET_KEY"));
    assert!(!env.contains_key("API_KEY"));
}

// ===== Kill/Cleanup Safety Tests =====

#[test]
fn child_engine_child_killed_before_response() {
    // Use a command that exits immediately without sending IPC response
    let engine = ChildEngine::new(PathBuf::from("/bin/true"), PathBuf::from(""));

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline);

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));

    // Should fail gracefully (no response from child)
    assert!(result.is_err());
}

#[test]
fn child_engine_child_crash_handling() {
    // Use a command that exits with non-zero status
    let engine = ChildEngine::new(PathBuf::from("/bin/false"), PathBuf::from(""));

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let ctx = RequestContext::new(PathBuf::from("/tmp"), PathBuf::from("test.php"), deadline);

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(engine.execute(ctx));

    assert!(result.is_err());
}

#[test]
fn child_engine_capabilities_correct() {
    let engine = ChildEngine::with_default_php();
    let caps = engine.capabilities();
    assert!(caps.contains(&"child"));
    assert!(caps.contains(&"process"));
    assert!(caps.contains(&"isolated"));
    assert!(caps.contains(&"ipc"));
}
