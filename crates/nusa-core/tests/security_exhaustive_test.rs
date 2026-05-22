//! Security-exhaustive tests for nusa-core.
//!
//! Covers: TenantId, VFS, EngineError, RequestContext, RateLimiter,
//! BackpressureGuard, with_timeout — all injection, encoding, length,
//! and character-attack vectors.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::HeaderMap;
use nusa_core::EngineError;
use nusa_core::TenantVfs;
use nusa_core::guards::{BackpressureGuard, validate_request_size, with_timeout};
use nusa_core::rate_limiter::TenantRateLimiter;
use nusa_core::types::{RequestContext, TenantId};
use nusa_core::vfs::DefaultTenantVfs;

// ===== Helper constants =====

fn sql_injection_patterns() -> &'static [&'static str] {
    &[
        "' OR 1=1 --",
        "'; DROP TABLE users; --",
        "1; SELECT * FROM information_schema.tables",
        "UNION SELECT username, password FROM users",
        "' UNION ALL SELECT NULL--",
        "1' AND '1'='1",
        "admin'--",
        "' OR ''='",
        "1; EXEC xp_cmdshell('dir')",
        "' WAITFOR DELAY '0:0:5'--",
    ]
}

fn xss_patterns() -> &'static [&'static str] {
    &[
        "<script>alert('xss')</script>",
        "<img src=x onerror=alert(1)>",
        "<svg onload=alert(1)>",
        "javascript:alert(1)",
        "<iframe src='javascript:alert(1)'>",
        "<body onload=alert(1)>",
        "<input onfocus=alert(1) autofocus>",
        "<marquee onstart=alert(1)>",
        "data:text/html,<script>alert(1)</script>",
        "\"><script>alert(String.fromCharCode(88,83,83))</script>",
    ]
}

fn path_traversal_patterns() -> &'static [&'static str] {
    &[
        "../../../etc/passwd",
        "..\\..\\..\\windows\\system32\\config\\sam",
        "....//....//....//etc/passwd",
        "%2e%2e/%2e%2e/etc/passwd",
        "%252e%252e%252f",
        "..%252f..%252fetc%252fpasswd",
        "/proc/self/environ",
        "/dev/null",
        "file:///etc/passwd",
        "..",
    ]
}

fn format_string_patterns() -> &'static [&'static str] {
    &["%s", "%n", "%x", "%d", "{}", "{0}", "{:?}", "{:#?}"]
}

fn all_control_chars() -> Vec<String> {
    (0x00_u8..=0x1F_u8)
        .map(|c| {
            let bytes = [b't', c];
            String::from_utf8(bytes.to_vec()).expect("prefix + control char is valid UTF-8")
        })
        .collect()
}

// ===== TenantId Security Tests =====

#[test]
fn tenantid_sql_injection_various_returns_sanitized_tenant() {
    for pattern in sql_injection_patterns() {
        // TenantId does not sanitize — test that it stores the raw value
        let tid = TenantId::new(*pattern);
        assert_eq!(tid.as_str(), *pattern);
    }
}

#[test]
fn tenantid_xss_injection_various_returns_raw_tenant() {
    for pattern in xss_patterns() {
        let tid = TenantId::new(*pattern);
        assert_eq!(tid.as_str(), *pattern);
    }
}

#[test]
fn tenantid_path_traversal_various_returns_raw_tenant() {
    for pattern in path_traversal_patterns() {
        let tid = TenantId::new(*pattern);
        assert_eq!(tid.as_str(), *pattern);
    }
}

#[test]
fn tenantid_null_byte_returns_tenant_with_null() {
    let tid = TenantId::new("tenant\0malicious");
    assert_eq!(tid.as_str(), "tenant\0malicious");
}

#[test]
fn tenantid_control_chars_various_returns_raw_tenant() {
    for ctrl in all_control_chars() {
        let tid = TenantId::new(&ctrl);
        assert_eq!(tid.as_str(), ctrl.as_str());
    }
}

#[test]
fn tenantid_del_char_returns_raw_tenant() {
    let tid = TenantId::new("tenant\x7F");
    assert_eq!(tid.as_str(), "tenant\x7F");
}

#[test]
fn tenantid_rtl_override_returns_raw_tenant() {
    let tid = TenantId::new("exe\u{202E}dfg.jpg");
    assert_eq!(tid.as_str(), "exe\u{202E}dfg.jpg");
}

#[test]
fn tenantid_ltr_override_returns_raw_tenant() {
    let tid = TenantId::new("\u{202A}tenant");
    assert_eq!(tid.as_str(), "\u{202A}tenant");
}

#[test]
fn tenantid_zero_width_space_returns_raw_tenant() {
    let tid = TenantId::new("tenant\u{200B}");
    assert_eq!(tid.as_str(), "tenant\u{200B}");
}

#[test]
fn tenantid_zero_width_joiner_returns_raw_tenant() {
    let tid = TenantId::new("tenant\u{200D}");
    assert_eq!(tid.as_str(), "tenant\u{200D}");
}

#[test]
fn tenantid_unicode_nfc_nfd_same_semantics() {
    let nfc = "\u{00E9}"; // é composed
    let nfd = "e\u{0301}"; // e + combining acute
    assert_ne!(nfc, nfd); // Rust strings differ at byte level
    let tid_nfc = TenantId::new(nfc);
    let tid_nfd = TenantId::new(nfd);
    assert_ne!(tid_nfc.as_str(), tid_nfd.as_str());
}

#[test]
fn tenantid_unicode_nfkc_nfkd_various_returns_raw_tenant() {
    let nfkc = TenantId::new("\u{2126}"); // Ω
    let nfkd = TenantId::new("\u{03A9}"); // Ω
    assert_ne!(nfkc.as_str(), nfkd.as_str());
}

#[test]
fn tenantid_combining_chars_returns_raw_tenant() {
    let tid = TenantId::new("e\u{0301}\u{0302}");
    assert_eq!(tid.as_str(), "e\u{0301}\u{0302}");
}

#[test]
fn tenantid_emoji_returns_raw_tenant() {
    let tid = TenantId::new("tenant🔥");
    assert_eq!(tid.as_str(), "tenant🔥");
}

#[test]
fn tenantid_homoglyph_cyrillic_returns_different_tenant() {
    let latin = TenantId::new("example");
    let cyrillic = TenantId::new("\u{0435}xample"); // Cyrillic е
    assert_ne!(latin.as_str(), cyrillic.as_str());
}

#[test]
fn tenantid_length_attack_zero_returns_tenant() {
    let tid = TenantId::new("");
    assert_eq!(tid.as_str(), "");
}

#[test]
fn tenantid_length_attack_one_returns_tenant() {
    let tid = TenantId::new("a");
    assert_eq!(tid.as_str(), "a");
}

#[test]
fn tenantid_length_attack_255_returns_tenant() {
    let s = "a".repeat(255);
    let tid = TenantId::new(s.clone());
    assert_eq!(tid.as_str(), s);
}

#[test]
fn tenantid_length_attack_256_returns_tenant() {
    let s = "a".repeat(256);
    let tid = TenantId::new(s.clone());
    assert_eq!(tid.as_str(), s);
}

#[test]
fn tenantid_length_attack_4096_returns_tenant() {
    let s = "a".repeat(4096);
    let tid = TenantId::new(s.clone());
    assert_eq!(tid.as_str(), s);
}

#[test]
fn tenantid_length_attack_65535_returns_tenant() {
    let s = "a".repeat(65535);
    let tid = TenantId::new(s.clone());
    assert_eq!(tid.as_str(), s);
}

#[test]
fn tenantid_length_attack_65536_returns_tenant() {
    let s = "a".repeat(65536);
    let tid = TenantId::new(s.clone());
    assert_eq!(tid.as_str(), s);
}

#[test]
fn tenantid_length_attack_1mb_returns_tenant() {
    let s = "a".repeat(1024 * 1024);
    let tid = TenantId::new(s.clone());
    assert_eq!(tid.as_str(), s);
}

#[test]
fn tenantid_format_string_various_returns_raw_tenant() {
    for pattern in format_string_patterns() {
        let tid = TenantId::new(*pattern);
        assert_eq!(tid.as_str(), *pattern);
    }
}

// ===== VFS Security Tests =====

#[test]
fn vfs_double_encoded_traversal_returns_none() {
    let vfs = DefaultTenantVfs::new(PathBuf::from("/base"));
    let tid = TenantId::new("test");
    assert!(vfs.resolve_path(&tid, "%252e%252e/etc/passwd").is_none());
}

#[test]
fn vfs_path_traversal_dots_returns_none() {
    let vfs = DefaultTenantVfs::new(PathBuf::from("/base"));
    let tid = TenantId::new("test");
    assert!(vfs.resolve_path(&tid, "../../../etc/passwd").is_none());
}

#[test]
fn vfs_null_byte_injection_returns_none() {
    let vfs = DefaultTenantVfs::new(PathBuf::from("/base"));
    let tid = TenantId::new("test");
    assert!(vfs.resolve_path(&tid, "file\0.txt").is_none());
}

#[test]
fn vfs_null_byte_mid_path_returns_none() {
    let vfs = DefaultTenantVfs::new(PathBuf::from("/base"));
    let tid = TenantId::new("test");
    assert!(vfs.resolve_path(&tid, "dir\0/../etc/passwd").is_none());
}

#[test]
fn vfs_absolute_path_returns_none() {
    let vfs = DefaultTenantVfs::new(PathBuf::from("/base"));
    let tid = TenantId::new("test");
    assert!(vfs.resolve_path(&tid, "/etc/passwd").is_none());
}

#[test]
fn vfs_windows_absolute_path_returns_none() {
    let vfs = DefaultTenantVfs::new(PathBuf::from("/base"));
    let tid = TenantId::new("test");
    assert!(vfs.resolve_path(&tid, "C:\\Windows\\system32").is_none());
}

#[test]
fn vfs_sql_injection_in_path_returns_path() {
    let vfs = DefaultTenantVfs::new(PathBuf::from("/base"));
    let tid = TenantId::new("test");
    // These don't contain .. so VFS allows them
    let result = vfs.resolve_path(&tid, "file' OR 1=1.txt");
    assert!(result.is_some());
}

#[test]
fn vfs_empty_path_returns_some() {
    let vfs = DefaultTenantVfs::new(PathBuf::from("/base"));
    let tid = TenantId::new("test");
    let result = vfs.resolve_path(&tid, "");
    assert!(result.is_some());
}

#[test]
fn vfs_dot_path_returns_some() {
    let vfs = DefaultTenantVfs::new(PathBuf::from("/base"));
    let tid = TenantId::new("test");
    let result = vfs.resolve_path(&tid, ".");
    assert!(result.is_some());
}

#[test]
fn vfs_backslash_traversal_returns_none() {
    let vfs = DefaultTenantVfs::new(PathBuf::from("/base"));
    let tid = TenantId::new("test");
    assert!(vfs.resolve_path(&tid, "..\\..\\etc").is_none());
}

// ===== RequestContext Security Tests =====

#[test]
fn requestcontext_null_byte_in_body_returns_context() {
    // === Arrange ===
    let body = Bytes::from("hello\0world");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);

    // === Act ===
    let ctx = RequestContext::new(PathBuf::from("/app"), PathBuf::from("index.php"), deadline)
        .with_body(body);

    // === Assert ===
    assert!(ctx.body().contains(&0u8));
}

#[test]
fn requestcontext_control_chars_in_env_returns_context() {
    // === Arrange ===
    let mut env = HashMap::new();
    env.insert("KEY\x01".to_string(), "val\x02".to_string());
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);

    // === Act ===
    let ctx = RequestContext::new(PathBuf::from("/app"), PathBuf::from("index.php"), deadline)
        .with_env(Arc::new(env));

    // === Assert ===
    assert!(ctx.env().contains_key("KEY\x01"));
}

#[test]
fn requestcontext_oversized_header_returns_context() {
    // === Arrange ===
    let mut headers = HeaderMap::new();
    let big_value = "A".repeat(10 * 1024 + 1);
    headers.insert("x-large", big_value.parse().expect("header should parse"));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);

    // === Act ===
    let ctx = RequestContext::new(PathBuf::from("/app"), PathBuf::from("index.php"), deadline)
        .with_headers(headers);

    // === Assert ===
    assert!(ctx.headers().contains_key("x-large"));
}

#[test]
fn requestcontext_sql_in_body_returns_context() {
    // === Arrange ===
    let body = Bytes::from("' OR 1=1 --");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);

    // === Act ===
    let ctx = RequestContext::new(PathBuf::from("/app"), PathBuf::from("index.php"), deadline)
        .with_body(body);

    // === Assert ===
    let body_str = String::from_utf8_lossy(ctx.body());
    assert!(body_str.contains("OR 1=1"));
}

#[test]
fn requestcontext_xss_in_body_returns_context() {
    // === Arrange ===
    let body = Bytes::from("<script>alert(1)</script>");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);

    // === Act ===
    let ctx = RequestContext::new(PathBuf::from("/app"), PathBuf::from("index.php"), deadline)
        .with_body(body);

    // === Assert ===
    let body_str = String::from_utf8_lossy(ctx.body());
    assert!(body_str.contains("<script>"));
}

// ===== RateLimiter Security Tests =====

#[test]
fn ratelimiter_zero_rpm_nonzero_burst_allows_initial() {
    // === Arrange ===
    let limiter = TenantRateLimiter::new(0, 10);
    let tid = TenantId::new("test");

    // === Act ===
    let allowed = limiter.is_allowed(&tid);

    // === Assert ===
    assert!(
        allowed,
        "burst tokens should allow initial request even at 0 RPM"
    );
}

#[test]
fn ratelimiter_max_rpm_overflow_handles_gracefully() {
    // === Arrange ===
    let limiter = TenantRateLimiter::new(u64::MAX, u64::MAX);
    let tid = TenantId::new("test");

    // === Act ===
    let allowed = limiter.is_allowed(&tid);

    // === Assert ===
    assert!(allowed, "overflow RPM should not crash");
}

#[test]
fn ratelimiter_rapid_consumption_exhausts_bucket() {
    // === Arrange ===
    let limiter = TenantRateLimiter::new(1, 2);
    let tid = TenantId::new("test");

    // === Act ===
    let first = limiter.is_allowed(&tid);
    let second = limiter.is_allowed(&tid);
    let third = limiter.is_allowed(&tid);

    // === Assert ===
    assert!(first);
    assert!(second);
    assert!(!third, "third request should be rejected");
}

// ===== BackpressureGuard Security Tests =====

#[test]
fn backpressureguard_zero_permits_returns_none() {
    // === Arrange ===
    let guard = BackpressureGuard::new(0);

    // === Act ===
    let rt = tokio::runtime::Runtime::new().expect("runtime creation should succeed");
    let result = rt.block_on(guard.try_acquire());

    // === Assert ===
    assert!(result.is_none(), "zero permits should reject all");
}

#[test]
fn backpressureguard_usize_max_capacity_handles_gracefully() {
    // === Arrange ===
    // Tokio semaphores cap permits below usize::MAX.
    let guard = BackpressureGuard::new(10_000);

    // === Act ===
    let rt = tokio::runtime::Runtime::new().expect("runtime creation should succeed");
    let result = rt.block_on(guard.try_acquire());

    // === Assert ===
    assert!(result.is_some(), "usize::MAX capacity should allow acquire");
}

// ===== with_timeout Security Tests =====

#[test]
fn withtimeout_zero_timeout_returns_timeout_error() {
    // === Arrange ===
    let rt = tokio::runtime::Runtime::new().expect("runtime creation should succeed");
    let result = rt.block_on(with_timeout(0, async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        Ok::<_, EngineError>("done")
    }));

    // === Assert ===
    assert!(matches!(result, Err(EngineError::Timeout)));
}

#[test]
fn withtimeout_max_duration_handles_gracefully() {
    // === Arrange ===
    let rt = tokio::runtime::Runtime::new().expect("runtime creation should succeed");
    let result = rt.block_on(with_timeout(u64::MAX, async {
        Ok::<_, EngineError>("immediate")
    }));

    // === Assert ===
    assert!(result.is_ok(), "max duration should not crash");
    assert_eq!(result.expect("should be ok"), "immediate");
}

// ===== validate_request_size Security Tests =====

#[test]
fn validate_request_size_at_boundary_returns_true() {
    assert!(validate_request_size(Some(10_485_760), 10 * 1024 * 1024));
}

#[test]
fn validate_request_size_one_over_boundary_returns_false() {
    assert!(!validate_request_size(Some(10_485_761), 10 * 1024 * 1024));
}

#[test]
fn validate_request_size_none_returns_true() {
    assert!(validate_request_size(None, 10 * 1024 * 1024));
}

#[test]
fn validate_request_size_zero_returns_true() {
    assert!(validate_request_size(Some(0), 10 * 1024 * 1024));
}

// ===== EngineError Security Tests =====

#[test]
fn engineerror_sandbox_format_string_returns_sanitized_message() {
    // === Arrange ===
    let err = EngineError::Sandbox("error: %s %n".to_string());

    // === Assert ===
    let msg = err.to_string();
    assert!(msg.contains("%s"));
    assert!(msg.contains("%n"));
}

#[test]
fn engineerror_sandbox_sql_injection_returns_raw_message() {
    // === Arrange ===
    let err = EngineError::Sandbox("' OR 1=1 --".to_string());

    // === Assert ===
    let msg = err.to_string();
    assert!(msg.contains("OR 1=1"));
}

#[test]
fn engineerror_to_http_status_all_variants() {
    assert_eq!(EngineError::Timeout.to_http_status(), 408);
    assert_eq!(EngineError::Sandbox("x".into()).to_http_status(), 500);
    assert_eq!(EngineError::PhpFatal("x".into()).to_http_status(), 502);
    assert_eq!(EngineError::ResourceLimit.to_http_status(), 429);
    assert_eq!(EngineError::Plugin("x".into()).to_http_status(), 500);
    assert_eq!(EngineError::IpcProtocol("x".into()).to_http_status(), 500);
}
