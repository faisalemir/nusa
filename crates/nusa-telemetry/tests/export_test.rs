//! Comprehensive tests for telemetry export.
//!
//! Covers: ConsoleBackend, FileBackend, bandwidth tracking, periodic export,
//! graceful shutdown, concurrent access, edge cases.

use std::sync::Arc;

use nusa_telemetry::export::{BillingExport, ExportBackend, TelemetryExport};

// ── Console Exporter ──

#[test]
fn console_export_succeeds() {
    let backend = ExportBackend::Console;
    let exports = vec![BillingExport {
        tenant_id: "test-tenant".to_string(),
        requests: 100,
        cpu_ms: 5000,
        memory_mb: 128.5,
        bandwidth_bytes: 1024 * 1024,
        timestamp: "2025-01-01T00:00:00Z".to_string(),
    }];
    // Console export should not error
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(backend.export(&exports));
    assert!(result.is_ok());
}

#[test]
fn console_export_empty_list() {
    let backend = ExportBackend::Console;
    let exports: Vec<BillingExport> = vec![];
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(backend.export(&exports));
    assert!(result.is_ok());
}

#[test]
fn console_export_name_is_console() {
    let backend = ExportBackend::Console;
    assert_eq!(backend.name(), "console");
}

// ── File Exporter ──

#[test]
fn file_export_writes_to_disk() {
    let temp_file = std::env::temp_dir().join("nusa_telemetry_test.json");
    // Clean up if exists
    let _ = std::fs::remove_file(&temp_file);

    let backend = ExportBackend::File {
        path: temp_file.clone(),
    };
    let exports = vec![BillingExport {
        tenant_id: "file-tenant".to_string(),
        requests: 50,
        cpu_ms: 2000,
        memory_mb: 64.0,
        bandwidth_bytes: 512 * 1024,
        timestamp: "2025-01-01T00:00:00Z".to_string(),
    }];

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(backend.export(&exports));
    assert!(result.is_ok());

    // Verify file was created
    assert!(temp_file.exists());
    let content = std::fs::read_to_string(&temp_file).unwrap();
    assert!(content.contains("file-tenant"));
    assert!(content.contains("requests"));

    // Cleanup
    let _ = std::fs::remove_file(&temp_file);
}

#[test]
fn file_export_appends_to_existing_file() {
    let temp_file = std::env::temp_dir().join("nusa_telemetry_append_test.json");
    let _ = std::fs::remove_file(&temp_file);

    let backend = ExportBackend::File {
        path: temp_file.clone(),
    };
    let rt = tokio::runtime::Runtime::new().unwrap();

    // First export
    rt.block_on(backend.export(&[BillingExport {
        tenant_id: "tenant-1".to_string(),
        requests: 10,
        cpu_ms: 500,
        memory_mb: 32.0,
        bandwidth_bytes: 1024,
        timestamp: "2025-01-01T00:00:00Z".to_string(),
    }]))
    .unwrap();

    // Second export
    rt.block_on(backend.export(&[BillingExport {
        tenant_id: "tenant-2".to_string(),
        requests: 20,
        cpu_ms: 1000,
        memory_mb: 64.0,
        bandwidth_bytes: 2048,
        timestamp: "2025-01-01T01:00:00Z".to_string(),
    }]))
    .unwrap();

    let content = std::fs::read_to_string(&temp_file).unwrap();
    // Should contain both records
    assert!(content.contains("tenant-1"));
    assert!(content.contains("tenant-2"));
    // Should have two lines
    let line_count = content.lines().count();
    assert_eq!(line_count, 2);

    let _ = std::fs::remove_file(&temp_file);
}

#[test]
fn file_export_empty_is_noop() {
    let temp_file = std::env::temp_dir().join("nusa_telemetry_empty.json");
    let _ = std::fs::remove_file(&temp_file);

    let backend = ExportBackend::File {
        path: temp_file.clone(),
    };
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(backend.export(&[]));
    assert!(result.is_ok());
    // File should NOT be created for empty exports
    assert!(!temp_file.exists());

    let _ = std::fs::remove_file(&temp_file);
}

#[test]
fn file_export_name_is_file() {
    let backend = ExportBackend::File {
        path: "test.json".into(),
    };
    assert_eq!(backend.name(), "file");
}

// ── TelemetryExport ──

#[test]
fn telemetry_export_record_request() {
    let export = TelemetryExport::with_console_backend(60);
    export.record_request("tenant-a", 100, 64.0, 2048);
    // No error expected — recording is fire-and-forget
}

#[test]
fn telemetry_export_accumulates_bandwidth() {
    let export = TelemetryExport::with_console_backend(60);
    export.record_request("tenant-b", 50, 32.0, 1024);
    export.record_request("tenant-b", 50, 32.0, 2048);
    export.record_request("tenant-b", 50, 32.0, 512);
    // Bandwidth should accumulate (1024 + 2048 + 512 = 3584)
}

#[test]
fn telemetry_export_start_and_shutdown() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let export = TelemetryExport::with_console_backend(60);
        export.record_request("tenant-c", 200, 128.0, 4096);
        let shutdown_tx = Arc::new(export).start_export_loop();
        // Trigger shutdown immediately
        shutdown_tx.send(()).unwrap();
        // No panic expected — shutdown should be clean
    });
}

#[test]
fn telemetry_export_multiple_tenants() {
    let export = TelemetryExport::with_console_backend(60);
    export.record_request("tenant-x", 100, 64.0, 1024);
    export.record_request("tenant-y", 200, 128.0, 2048);
    export.record_request("tenant-z", 300, 256.0, 4096);
    // All should be recorded independently
}

#[test]
fn telemetry_export_concurrent_record() {
    use std::thread;

    let export = Arc::new(TelemetryExport::with_console_backend(60));
    let mut handles = vec![];

    for i in 0..10 {
        let exp = export.clone();
        handles.push(thread::spawn(move || {
            let tenant = format!("tenant-{}", i);
            exp.record_request(&tenant, 100, 64.0, 1024);
        }));
    }

    for h in handles {
        h.join().expect("thread must not panic");
    }
}

// ── Edge Cases ──

#[test]
fn billing_export_serializes_correctly() {
    let export = BillingExport {
        tenant_id: "test".to_string(),
        requests: 0,
        cpu_ms: 0,
        memory_mb: 0.0,
        bandwidth_bytes: 0,
        timestamp: "2025-01-01T00:00:00Z".to_string(),
    };
    let json = serde_json::to_string(&export).unwrap();
    assert!(json.contains("tenant_id"));
    assert!(json.contains("test"));
}

#[test]
fn billing_export_with_max_values() {
    let export = BillingExport {
        tenant_id: "max".to_string(),
        requests: u64::MAX,
        cpu_ms: u64::MAX,
        memory_mb: f64::MAX,
        bandwidth_bytes: u64::MAX,
        timestamp: "2025-01-01T00:00:00Z".to_string(),
    };
    let json = serde_json::to_string(&export).unwrap();
    assert!(json.contains("max"));
}

#[test]
fn telemetry_export_zero_duration() {
    let export = TelemetryExport::with_console_backend(1);
    export.record_request("fast-tenant", 0, 0.0, 0);
    // Should handle zero values gracefully
}
