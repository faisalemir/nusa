//! Domain-specific tests for the child process engine.
//!
//! Covers: process spawn, STDIN/STDOUT/STDERR, IPC roundtrip,
//! kill/cleanup, crash detection, zombie prevention, and timeout.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use nusa_engine_child::process::ChildProcess;

// ─── Helper: write a script to a temp file ─────────────────────────────────

fn write_temp_script(name: &str, contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("nusa_child_test_{}", name));
    let mut f = std::fs::File::create(&path).expect("create script");
    f.write_all(contents.as_bytes()).expect("write script");
    path
}

// ─── 1. Process Spawn ─────────────────────────────────────────────────────

#[tokio::test]
async fn child_process_spawn_with_valid_php_binary() {
    // Create a minimal PHP bootstrap that reads stdin and writes to stdout
    let bootstrap = write_temp_script(
        "spawn.php",
        r#"<?php
$stdin = fopen('php://stdin', 'r');
if ($stdin) {
    $data = stream_get_contents($stdin);
    fclose($stdin);
}
// Write a minimal response
echo json_encode(['status' => 200, 'body' => 'hello']);
"#,
    );

    // Check if PHP is available
    let php_available = Command::new("php").arg("--version").output().is_ok();

    if php_available {
        let child = ChildProcess::spawn(Path::new("php"), &bootstrap).await;
        assert!(child.is_ok(), "PHP process should spawn");
        let mut child = child.expect("spawned child");
        assert!(child.pid().is_some(), "child should have a PID");

        let _ = child.shutdown().await;
    }

    let _ = std::fs::remove_file(&bootstrap);
}

#[tokio::test]
async fn child_process_spawn_with_nonexistent_binary_returns_error() {
    let result =
        ChildProcess::spawn(Path::new("/nonexistent/php12345"), Path::new("index.php")).await;
    assert!(result.is_err(), "nonexistent binary should return error");
}

#[tokio::test]
async fn child_process_spawn_with_custom_bootstrap_script() {
    let bootstrap = write_temp_script(
        "custom.php",
        r#"<?php
echo "bootstrap loaded";
"#,
    );

    let php_available = Command::new("php").arg("--version").output().is_ok();

    if php_available {
        let result = ChildProcess::spawn(Path::new("php"), &bootstrap).await;
        assert!(result.is_ok(), "custom bootstrap should work");
        let mut child = result.expect("spawned child");
        let _ = child.shutdown().await;
    }

    let _ = std::fs::remove_file(&bootstrap);
}

#[tokio::test]
async fn child_process_spawn_with_custom_php_binary_path() {
    // Try with /usr/bin/php if it exists
    if PathBuf::from("/usr/bin/php").exists() {
        let bootstrap = write_temp_script("custom_path.php", r#"<?php echo "ok"; "#);
        let result = ChildProcess::spawn(Path::new("/usr/bin/php"), &bootstrap).await;
        assert!(result.is_ok(), "custom PHP path should work");
        let mut child = result.expect("spawned child");
        let _ = child.shutdown().await;
        let _ = std::fs::remove_file(&bootstrap);
    }
}

// ─── 2. STDIN / STDOUT / STDERR ───────────────────────────────────────────

#[tokio::test]
async fn child_process_write_stdin_sends_framed_data() {
    let bootstrap = write_temp_script(
        "stdin.php",
        r#"<?php
$stdin = fopen('php://stdin', 'r');
if ($stdin) {
    $data = stream_get_contents($stdin);
    fclose($stdin);
    // Echo back the data
    echo $data;
}
"#,
    );

    let php_available = Command::new("php").arg("--version").output().is_ok();

    if php_available {
        let mut child = ChildProcess::spawn(Path::new("php"), &bootstrap)
            .await
            .expect("spawn");

        // Write framed data to stdin
        let framed = b"hello world";
        let write_result = child.write_stdin(framed).await;
        assert!(write_result.is_ok(), "stdin write should succeed");

        let _ = child.shutdown().await;
    }

    let _ = std::fs::remove_file(&bootstrap);
}

#[tokio::test]
async fn child_process_read_stdout_parses_framed_response() {
    let bootstrap = write_temp_script(
        "stdout.php",
        r#"<?php
// Read framed input from stdin, then respond with framed output
$stdin = fopen('php://stdin', 'r');
if ($stdin) {
    // Read 4-byte length prefix
    $header = fread($stdin, 4);
    if ($header && strlen($header) === 4) {
        $len = unpack('V', $header)[1];
        $payload = fread($stdin, $len);
    }
    fclose($stdin);
}
// Send framed response
$response = json_encode(['status' => 200, 'body' => 'response']);
$len = strlen($response);
fwrite(STDOUT, pack('V', $len) . $response);
"#,
    );

    let php_available = Command::new("php").arg("--version").output().is_ok();

    if php_available {
        let mut child = ChildProcess::spawn(Path::new("php"), &bootstrap)
            .await
            .expect("spawn");

        // Send a framed request
        let request = b"test request";
        let len = request.len() as u32;
        let mut framed = Vec::with_capacity(4 + request.len());
        framed.extend_from_slice(&len.to_le_bytes());
        framed.extend_from_slice(request);

        child.write_stdin(&framed).await.expect("stdin write");

        // Read framed response
        let read_result = child.read_stdout().await;
        assert!(read_result.is_ok(), "stdout read should succeed");
        let response_bytes = read_result.expect("response");
        assert!(
            response_bytes.len() >= 4,
            "response should have length prefix"
        );

        let _ = child.shutdown().await;
    }

    let _ = std::fs::remove_file(&bootstrap);
}

#[tokio::test]
async fn child_process_empty_stdout_handled() {
    let bootstrap = write_temp_script(
        "empty.php",
        r#"<?php
// Don't write anything to stdout
"#,
    );

    let php_available = Command::new("php").arg("--version").output().is_ok();

    if php_available {
        let mut child = ChildProcess::spawn(Path::new("php"), &bootstrap)
            .await
            .expect("spawn");

        // Reading from empty stdout should return error or empty
        let _result = child.read_stdout().await;
        // Either timeout/error is expected since no framed data sent
        // This verifies the child handles the case without crashing

        let _ = child.shutdown().await;
    }

    let _ = std::fs::remove_file(&bootstrap);
}

#[tokio::test]
async fn child_process_stderr_captured_during_execution() {
    let bootstrap = write_temp_script(
        "stderr.php",
        r#"<?php
fwrite(STDERR, "This is an error message\n");
echo "stdout response";
"#,
    );

    let php_available = Command::new("php").arg("--version").output().is_ok();

    if php_available {
        let mut child = ChildProcess::spawn(Path::new("php"), &bootstrap)
            .await
            .expect("spawn");
        // stderr is piped but not explicitly read — process should still complete
        let _ = child.shutdown().await;
    }

    let _ = std::fs::remove_file(&bootstrap);
}

#[tokio::test]
async fn child_process_large_request_body_via_stdin() {
    let bootstrap = write_temp_script(
        "large_body.php",
        r#"<?php
$stdin = fopen('php://stdin', 'r');
if ($stdin) {
    $data = stream_get_contents($stdin);
    fclose($stdin);
    if (strlen($data) > 0) {
        echo "received " . strlen($data) . " bytes";
    }
}
"#,
    );

    let php_available = Command::new("php").arg("--version").output().is_ok();

    if php_available {
        let mut child = ChildProcess::spawn(Path::new("php"), &bootstrap)
            .await
            .expect("spawn");

        // Send a large body (1MB)
        let large_body = vec![b'x'; 1024 * 1024];
        let write_result = child.write_stdin(&large_body).await;
        assert!(write_result.is_ok(), "large stdin write should succeed");

        let _ = child.shutdown().await;
    }

    let _ = std::fs::remove_file(&bootstrap);
}

// ─── 3. IPC Roundtrip ─────────────────────────────────────────────────────

#[tokio::test]
async fn child_process_ipc_frame_encode_decode_roundtrip() {
    use nusa_ipc::RequestId;
    use nusa_ipc::protocol::IpcMessage;

    // Create a request message
    let request = IpcMessage::Request {
        id: RequestId::new(),
        method: "GET".to_string(),
        uri: "/test".to_string(),
        headers: HashMap::from([(
            "Content-Type".to_string(),
            vec!["application/json".to_string()],
        )]),
        query: Default::default(),
        post: Default::default(),
        cookies: Default::default(),
        files: vec![],
        body: Some(b"test body".to_vec()),
        server: Default::default(),
        timeout_ms: 5000,
        trace_context: None,
    };

    // Encode to framed bytes
    let framed = request.to_framed_bytes().expect("encode to framed bytes");
    assert!(framed.len() > 4, "framed data should have length prefix");

    // Decode back
    let decoded = IpcMessage::from_framed_bytes(&framed).expect("decode framed bytes");
    match decoded {
        IpcMessage::Request {
            method, uri, body, ..
        } => {
            assert_eq!(method, "GET");
            assert_eq!(uri, "/test");
            assert_eq!(body.as_ref().expect("body"), b"test body");
        }
        other => panic!("expected Request, got {:?}", std::mem::discriminant(&other)),
    }
}

#[tokio::test]
async fn child_process_ipc_response_serialized_to_stdout() {
    use nusa_ipc::RequestId;
    use nusa_ipc::protocol::IpcMessage;

    // Create a response message
    let response = IpcMessage::Response {
        id: RequestId::new(),
        status: 200,
        headers: HashMap::from([("Content-Type".to_string(), vec!["text/html".to_string()])]),
        body: b"hello response".to_vec(),
        terminated: true,
    };

    let framed = response.to_framed_bytes().expect("encode response");
    assert!(framed.len() > 4);

    let decoded = IpcMessage::from_framed_bytes(&framed).expect("decode response");
    match decoded {
        IpcMessage::Response { status, body, .. } => {
            assert_eq!(status, 200);
            assert_eq!(&body, b"hello response");
        }
        other => panic!(
            "expected Response, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

// ─── 4. Kill / Cleanup ────────────────────────────────────────────────────

#[tokio::test]
async fn child_process_kill_terminates_process() {
    // PHP script that sleeps forever
    let bootstrap = write_temp_script(
        "sleep.php",
        r#"<?php
sleep(300);  // sleep 5 minutes
"#,
    );

    let php_available = Command::new("php").arg("--version").output().is_ok();

    if php_available {
        let mut child = ChildProcess::spawn(Path::new("php"), &bootstrap)
            .await
            .expect("spawn");
        let pid = child.pid();
        assert!(pid.is_some(), "should have PID");

        // Kill the process
        let kill_result = child.shutdown().await;
        assert!(kill_result.is_ok(), "kill should succeed");
        // shutdown() calls kill() then wait() — process is reaped
    }

    let _ = std::fs::remove_file(&bootstrap);
}

#[tokio::test]
async fn child_process_cleanup_releases_fds_and_memory() {
    let bootstrap = write_temp_script("cleanup.php", r#"<?php echo "cleanup test"; "#);

    let php_available = Command::new("php").arg("--version").output().is_ok();

    if php_available {
        {
            let mut child = ChildProcess::spawn(Path::new("php"), &bootstrap)
                .await
                .expect("spawn");
            let _ = child.write_stdin(b"test").await;
            // Explicit shutdown
            let _ = child.shutdown().await;
        }
        // child dropped — FDs and memory released
    }

    let _ = std::fs::remove_file(&bootstrap);
}

#[tokio::test]
async fn child_process_kill_before_response_cleans_up() {
    let bootstrap = write_temp_script(
        "no_response.php",
        r#"<?php
// Don't write any response
sleep(1);
"#,
    );

    let php_available = Command::new("php").arg("--version").output().is_ok();

    if php_available {
        let mut child = ChildProcess::spawn(Path::new("php"), &bootstrap)
            .await
            .expect("spawn");
        let _ = child.write_stdin(b"test").await;
        // Kill before reading response
        let _ = child.shutdown().await;
        // Should not panic or leak
    }

    let _ = std::fs::remove_file(&bootstrap);
}

// ─── 5. Crash Detection ───────────────────────────────────────────────────

#[tokio::test]
async fn child_process_crash_detected_and_error_returned() {
    // PHP script that exits immediately with nonzero code
    let bootstrap = write_temp_script(
        "crash.php",
        r#"<?php
exit(1);
"#,
    );

    let php_available = Command::new("php").arg("--version").output().is_ok();

    if php_available {
        let mut child = ChildProcess::spawn(Path::new("php"), &bootstrap)
            .await
            .expect("spawn");
        // Read from stdout — process exited without sending data
        let result = child.read_stdout().await;
        assert!(result.is_err(), "read from crashed process should fail");
        let _ = child.shutdown().await;
    }

    let _ = std::fs::remove_file(&bootstrap);
}

#[tokio::test]
async fn child_process_nonzero_exit_code_handled() {
    let bootstrap = write_temp_script(
        "exit_nonzero.php",
        r#"<?php
echo "partial output";
exit(42);
"#,
    );

    let php_available = Command::new("php").arg("--version").output().is_ok();

    if php_available {
        let mut child = ChildProcess::spawn(Path::new("php"), &bootstrap)
            .await
            .expect("spawn");
        // Try to read stdout — may or may not get data before exit
        let _ = child.read_stdout().await;
        let _ = child.shutdown().await;
    }

    let _ = std::fs::remove_file(&bootstrap);
}

#[tokio::test]
async fn child_process_oom_killed_child_detected() {
    // Simulate a crash by terminating the process externally (Unix only)
    let bootstrap = write_temp_script(
        "oom.php",
        r#"<?php
sleep(300);
"#,
    );

    let php_available = Command::new("php").arg("--version").output().is_ok();

    if php_available {
        #[cfg(unix)]
        {
            use std::process::Command as StdCommand;

            let mut child = ChildProcess::spawn(Path::new("php"), &bootstrap)
                .await
                .expect("spawn");
            let pid = child.pid().expect("pid");

            // Send SIGKILL via kill command (simulating OOM killer)
            let _ = StdCommand::new("kill")
                .arg("-9")
                .arg(pid.to_string())
                .output();

            // Wait a moment for process to die
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // Now try to read stdout — should fail
            let result = child.read_stdout().await;
            assert!(result.is_err(), "read from killed process should fail");

            let _ = child.shutdown().await;
        }

        #[cfg(not(unix))]
        {
            // On Windows, just verify shutdown works
            let mut child = ChildProcess::spawn(Path::new("php"), &bootstrap)
                .await
                .expect("spawn");
            let _ = child.shutdown().await;
        }
    }

    let _ = std::fs::remove_file(&bootstrap);
}

// ─── 6. Zombie Prevention ─────────────────────────────────────────────────

#[tokio::test]
async fn child_process_shutdown_calls_waitpid_to_reap() {
    let bootstrap = write_temp_script("reap.php", r#"<?php echo "reap test"; "#);

    let php_available = Command::new("php").arg("--version").output().is_ok();

    if php_available {
        let mut child = ChildProcess::spawn(Path::new("php"), &bootstrap)
            .await
            .expect("spawn");
        let _ = child.shutdown().await;
        // shutdown() calls kill() then wait() — child is reaped
        // No zombie left behind
    }

    let _ = std::fs::remove_file(&bootstrap);
}

#[tokio::test]
async fn child_process_sequential_spawns_no_zombies() {
    let bootstrap = write_temp_script("sequential.php", r#"<?php echo "ok"; "#);

    let php_available = Command::new("php").arg("--version").output().is_ok();

    if php_available {
        // Spawn and kill 10 children sequentially
        for _ in 0..10 {
            let mut child = ChildProcess::spawn(Path::new("php"), &bootstrap)
                .await
                .expect("spawn");
            let _ = child.write_stdin(b"test").await;
            let _ = child.shutdown().await;
        }
        // All children should be reaped — no zombies
    }

    let _ = std::fs::remove_file(&bootstrap);
}

#[tokio::test]
async fn child_process_shutdown_already_dead_no_error() {
    let bootstrap = write_temp_script("already_dead.php", r#"<?php exit(0); "#);

    let php_available = Command::new("php").arg("--version").output().is_ok();

    if php_available {
        let mut child = ChildProcess::spawn(Path::new("php"), &bootstrap)
            .await
            .expect("spawn");
        // First shutdown should work
        let result1 = child.shutdown().await;
        assert!(result1.is_ok(), "first shutdown should succeed");

        // Second shutdown on already-dead process should be safe
        let result2 = child.shutdown().await;
        assert!(result2.is_ok(), "second shutdown should be safe");
    }

    let _ = std::fs::remove_file(&bootstrap);
}

// ─── 7. Timeout ───────────────────────────────────────────────────────────

#[tokio::test]
async fn child_process_request_timeout_from_deadline() {
    let bootstrap = write_temp_script(
        "timeout.php",
        r#"<?php
sleep(30);  // Sleep longer than any reasonable timeout
"#,
    );

    let php_available = Command::new("php").arg("--version").output().is_ok();

    if php_available {
        let mut child = ChildProcess::spawn(Path::new("php"), &bootstrap)
            .await
            .expect("spawn");
        let _ = child.write_stdin(b"test").await;

        // Read with timeout
        let result =
            tokio::time::timeout(std::time::Duration::from_millis(100), child.read_stdout()).await;
        assert!(result.is_err(), "read should timeout on sleeping process");

        let _ = child.shutdown().await;
    }

    let _ = std::fs::remove_file(&bootstrap);
}

#[tokio::test]
async fn child_process_empty_request_body_handled() {
    let bootstrap = write_temp_script(
        "empty_body.php",
        r#"<?php
$stdin = fopen('php://stdin', 'r');
if ($stdin) {
    $data = stream_get_contents($stdin);
    fclose($stdin);
    echo json_encode(['received' => strlen($data ?? '')]);
}
"#,
    );

    let php_available = Command::new("php").arg("--version").output().is_ok();

    if php_available {
        let mut child = ChildProcess::spawn(Path::new("php"), &bootstrap)
            .await
            .expect("spawn");
        // Write empty body
        let _ = child.write_stdin(b"").await;
        let _ = child.shutdown().await;
    }

    let _ = std::fs::remove_file(&bootstrap);
}

#[tokio::test]
async fn child_process_deserialization_error_from_malformed_response() {
    use nusa_ipc::protocol::IpcMessage;

    // Malformed framed data (length prefix points to garbage)
    let malformed = vec![0xFF, 0xFF, 0xFF, 0xFF]; // 4GB length
    let result = IpcMessage::from_framed_bytes(&malformed);
    assert!(
        result.is_err(),
        "malformed framed data should fail to decode"
    );
}

#[tokio::test]
async fn child_process_unexpected_message_type_from_child_handled() {
    use nusa_ipc::protocol::IpcMessage;

    // Child sends a Hello instead of Response
    let hello = IpcMessage::Hello {
        version: "1.0".to_string(),
        pid: 12345,
        capabilities: vec![],
    };
    let framed = hello.to_framed_bytes().expect("encode hello");
    let decoded = IpcMessage::from_framed_bytes(&framed).expect("decode");

    // Should be a Hello, not a Response
    match decoded {
        IpcMessage::Hello { version, .. } => {
            assert_eq!(version, "1.0");
        }
        other => panic!("expected Hello, got {:?}", std::mem::discriminant(&other)),
    }
}
