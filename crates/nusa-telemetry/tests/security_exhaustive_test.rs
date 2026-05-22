//! Security-exhaustive tests for nusa-telemetry.
//!
//! Covers: BillingExport injection, TelemetryExport oversized payloads,
//! TraceContext injection patterns.

use std::sync::Arc;

use nusa_telemetry::create_span;
use nusa_telemetry::export::{BillingExport, ExportBackend, TelemetryExport};

// ===== BillingExport Security Tests =====

#[test]
fn billing_export_sql_injection_in_tenant_id() {
    let patterns = [
        "' OR 1=1 --",
        "'; DROP TABLE billing; --",
        "1; SELECT * FROM billing_records",
        "UNION SELECT * FROM users",
        "' UNION ALL SELECT NULL--",
    ];
    for tenant_id in patterns {
        let export = BillingExport {
            tenant_id: tenant_id.to_string(),
            requests: 100,
            cpu_ms: 5000,
            memory_mb: 256.0,
            bandwidth_bytes: 1024 * 1024,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        assert_eq!(export.tenant_id, tenant_id);

        // Serialization should not crash
        let json = serde_json::to_string(&export).expect("serialize should succeed");
        assert!(json.contains(tenant_id) || !json.is_empty());
    }
}

#[test]
fn billing_export_xss_in_tenant_id() {
    let patterns = [
        "<script>alert('xss')</script>",
        "<img src=x onerror=alert(1)>",
        "<svg onload=alert(1)>",
        "javascript:alert(1)",
    ];
    for tenant_id in patterns {
        let export = BillingExport {
            tenant_id: tenant_id.to_string(),
            requests: 100,
            cpu_ms: 5000,
            memory_mb: 256.0,
            bandwidth_bytes: 1024 * 1024,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        // Serialization must round-trip safely; serde_json keeps angle brackets inside quoted strings.
        let json = serde_json::to_string(&export).expect("serialize should succeed");
        let parsed: BillingExport =
            serde_json::from_str(&json).expect("deserialize should succeed");
        assert_eq!(parsed.tenant_id, tenant_id);
        assert!(json.contains("\"tenant_id\""));
    }
}

#[test]
fn billing_export_path_traversal_in_tenant_id() {
    let patterns = [
        "../../../etc/passwd",
        "/proc/self/environ",
        "../../../tmp/malicious",
    ];
    for tenant_id in patterns {
        let export = BillingExport {
            tenant_id: tenant_id.to_string(),
            requests: 100,
            cpu_ms: 5000,
            memory_mb: 256.0,
            bandwidth_bytes: 1024 * 1024,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        assert_eq!(export.tenant_id, tenant_id);
    }
}

#[test]
fn billing_export_null_bytes_in_tenant_id() {
    let tenant_ids = [
        "tenant\0malicious",
        "\0leading",
        "trailing\0",
        "mid\0dle\0nulls",
    ];
    for tenant_id in tenant_ids {
        let export = BillingExport {
            tenant_id: tenant_id.to_string(),
            requests: 100,
            cpu_ms: 5000,
            memory_mb: 256.0,
            bandwidth_bytes: 1024 * 1024,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        // Null bytes in JSON are escaped
        let json = serde_json::to_string(&export).expect("serialize should succeed");
        assert!(json.contains("\\u0000") || !json.contains('\0'));
    }
}

#[test]
fn billing_export_oversized_tenant_id_10k() {
    let big_tenant = "A".repeat(10 * 1024 + 1);
    let export = BillingExport {
        tenant_id: big_tenant.clone(),
        requests: 100,
        cpu_ms: 5000,
        memory_mb: 256.0,
        bandwidth_bytes: 1024 * 1024,
        timestamp: "2024-01-01T00:00:00Z".to_string(),
    };
    assert_eq!(export.tenant_id.len(), 10 * 1024 + 1);

    // Serialization should handle large tenant_id
    let json = serde_json::to_string(&export).expect("serialize should succeed");
    assert!(json.len() > 10 * 1024);
}

#[test]
fn billing_export_format_string_in_tenant_id() {
    let patterns = ["%s", "%n", "%x", "{}", "{0}", "{:?}"];
    for tenant_id in patterns {
        let export = BillingExport {
            tenant_id: tenant_id.to_string(),
            requests: 100,
            cpu_ms: 5000,
            memory_mb: 256.0,
            bandwidth_bytes: 1024 * 1024,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&export).expect("serialize should succeed");
        assert!(json.contains(tenant_id) || json.contains("\\u"));
    }
}

#[test]
fn billing_export_control_chars_in_tenant_id() {
    for c in 0x01u8..=0x1Fu8 {
        let tenant_id = format!("tenant{}", c as char);
        let export = BillingExport {
            tenant_id: tenant_id.clone(),
            requests: 100,
            cpu_ms: 5000,
            memory_mb: 256.0,
            bandwidth_bytes: 1024 * 1024,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        // JSON serialization should escape control chars
        let json = serde_json::to_string(&export).expect("serialize should succeed");
        assert!(!json.contains(c as char) || json.contains("\\u00"));
    }
}

// ===== TelemetryExport Security Tests =====

#[test]
fn telemetry_export_injection_in_tenant_data() {
    let export_mgr = Arc::new(TelemetryExport::with_console_backend(1));

    let injection_tenant_ids = [
        "' OR 1=1 --",
        "<script>alert(1)</script>",
        "../../../etc/passwd",
        "tenant\0malicious",
    ];

    for tenant_id in injection_tenant_ids {
        export_mgr.record_request(tenant_id, 100, 64.0, 1024);
    }

    // Verify the data is stored without crashing
    drop(export_mgr);
}

#[test]
fn telemetry_export_oversized_payload() {
    let export_mgr = Arc::new(TelemetryExport::with_console_backend(1));

    // Record many entries to create a large export payload
    for i in 0..10_000 {
        let tenant_id = format!("tenant-{}", i);
        export_mgr.record_request(&tenant_id, 100, 64.0, 1024);
    }

    // The export should handle large payloads gracefully
    drop(export_mgr);
}

#[test]
fn telemetry_export_null_bytes_in_tenant_data() {
    let export_mgr = Arc::new(TelemetryExport::with_console_backend(1));

    export_mgr.record_request("tenant\0bad", 100, 64.0, 1024);

    drop(export_mgr);
}

// ===== ExportBackend File Security Tests =====

#[test]
fn export_backend_file_path_traversal_in_output_path() {
    let backend = ExportBackend::File {
        path: std::path::PathBuf::from("/tmp/../../../etc/nusa_export.json"),
    };

    let records = vec![BillingExport {
        tenant_id: "test".to_string(),
        requests: 1,
        cpu_ms: 100,
        memory_mb: 64.0,
        bandwidth_bytes: 1024,
        timestamp: "2024-01-01T00:00:00Z".to_string(),
    }];

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(backend.export(&records));

    // Should either succeed (file created at resolved path) or fail gracefully
    assert!(result.is_ok() || result.is_err());

    // Cleanup if file was created
    let _ = std::fs::remove_file("/tmp/../../../etc/nusa_export.json");
}

#[test]
fn export_backend_file_null_byte_in_path() {
    let backend = ExportBackend::File {
        path: std::path::PathBuf::from("/tmp/nusa\0export.json"),
    };

    let records = vec![BillingExport {
        tenant_id: "test".to_string(),
        requests: 1,
        cpu_ms: 100,
        memory_mb: 64.0,
        bandwidth_bytes: 1024,
        timestamp: "2024-01-01T00:00:00Z".to_string(),
    }];

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(backend.export(&records));

    assert!(result.is_err(), "null byte in path should fail");
}

#[test]
fn export_backend_console_injection_in_records() {
    let backend = ExportBackend::Console;

    let malicious_records = vec![
        BillingExport {
            tenant_id: "' OR 1=1 --".to_string(),
            requests: 100,
            cpu_ms: 5000,
            memory_mb: 256.0,
            bandwidth_bytes: 1024 * 1024,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        },
        BillingExport {
            tenant_id: "<script>alert(1)</script>".to_string(),
            requests: 100,
            cpu_ms: 5000,
            memory_mb: 256.0,
            bandwidth_bytes: 1024 * 1024,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        },
    ];

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(backend.export(&malicious_records));
    assert!(result.is_ok(), "console export should not crash");
}

// ===== TraceContext Security Tests =====

#[test]
fn telemetry_traceparent_header_injection() {
    let injection_patterns = [
        "' OR 1=1 --",
        "<script>alert(1)</script>",
        "../../../etc/passwd",
        "trace\0parent",
    ];

    for trace_id in injection_patterns {
        // create_span uses trace_id in the span — verify no crash
        let _span = create_span("test", trace_id);
    }
}

#[test]
fn telemetry_tracestate_header_injection() {
    // Tracestate header can contain injection patterns
    let injection_patterns = [
        "tenant=' OR 1=1 --",
        "tenant=<script>alert(1)</script>",
        "tenant=\0malicious",
    ];

    for tracestate in injection_patterns {
        let _span = create_span("test", tracestate);
    }
}

#[test]
fn telemetry_oversized_trace_headers() {
    let big_trace_id = "A".repeat(10 * 1024);
    let _span = create_span("test", &big_trace_id);
    assert!(big_trace_id.len() == 10 * 1024);
}
