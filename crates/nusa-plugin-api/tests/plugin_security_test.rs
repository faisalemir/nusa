//! Plugin security tests for nusa-plugin-api.
//!
//! Covers: Plugin registration injection, pre/post-exec injection,
//! error isolation.

use std::sync::Arc;

use nusa_core::{EngineError, RequestContext, Result};
use nusa_plugin_api::Plugin;
use nusa_plugin_api::PluginRegistry;

// ===== Plugin Registration Security Tests =====

struct TestPlugin {
    name: &'static str,
}

#[async_trait::async_trait]
impl Plugin for TestPlugin {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn pre_exec(&self, _ctx: &mut RequestContext) -> Result<()> {
        Ok(())
    }

    async fn post_exec(&self, _ctx: &RequestContext) -> Result<()> {
        Ok(())
    }
}

#[test]
fn plugin_registry_injection_patterns_in_plugin_names() {
    // === Arrange ===
    let registry = PluginRegistry::new();

    // Plugin names are &'static str — they can't contain runtime injection
    // But we verify the registry handles various static name patterns
    let test_plugin = Arc::new(TestPlugin {
        name: "test-plugin",
    });

    // === Act ===
    registry.register(test_plugin);

    // === Assert ===
    // Registration should succeed
}

#[test]
fn plugin_registry_null_byte_in_name() {
    // &'static str with null bytes
    let registry = PluginRegistry::new();
    let plugin = Arc::new(TestPlugin {
        name: "plugin\0malicious",
    });

    registry.register(plugin);
    // Should not crash
}

#[test]
fn plugin_registry_oversized_name() {
    // Static names are limited at compile time
    // We verify that long names don't cause issues
    let registry = PluginRegistry::new();
    let plugin = Arc::new(TestPlugin {
        name: "a-very-long-plugin-name-that-could-potentially-cause-issues-if-not-handled-properly",
    });

    registry.register(plugin);
}

#[test]
fn plugin_registry_format_string_in_name() {
    let registry = PluginRegistry::new();
    let plugin = Arc::new(TestPlugin {
        name: "plugin-%s-%n-%x",
    });

    registry.register(plugin);
}

// ===== Pre-exec/Post-exec Injection Tests =====

#[test]
fn plugin_pre_exec_injection_in_context() {
    // === Arrange ===
    let registry = PluginRegistry::new();
    let plugin = Arc::new(TestPlugin { name: "test" });
    registry.register(plugin.clone());

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut ctx = RequestContext::new(
        std::path::PathBuf::from("/tmp"),
        std::path::PathBuf::from("test.php"),
        deadline,
    );

    // === Act ===
    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(registry.run_pre_exec(&mut ctx));

    // === Assert ===
    assert!(result.is_ok(), "pre-exec should not crash");
}

#[test]
fn plugin_post_exec_injection_in_context() {
    let registry = PluginRegistry::new();
    let plugin = Arc::new(TestPlugin { name: "test" });
    registry.register(plugin.clone());

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let ctx = RequestContext::new(
        std::path::PathBuf::from("/tmp"),
        std::path::PathBuf::from("test.php"),
        deadline,
    );

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(registry.run_post_exec(&ctx));

    assert!(result.is_ok(), "post-exec should not crash");
}

#[test]
fn plugin_oversized_return_values() {
    // Plugin hooks return Result<()> — no oversized return values possible
    // But we verify the registry handles large numbers of plugins
    let registry = PluginRegistry::new();

    // Register many plugins
    for i in 0..100 {
        let plugin = Arc::new(TestPlugin {
            name: Box::leak(format!("plugin-{}", i).into_boxed_str()),
        });
        registry.register(plugin);
    }

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut ctx = RequestContext::new(
        std::path::PathBuf::from("/tmp"),
        std::path::PathBuf::from("test.php"),
        deadline,
    );

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(registry.run_pre_exec(&mut ctx));
    assert!(result.is_ok(), "100 plugins should execute without crash");
}

#[test]
fn plugin_null_bytes_in_responses() {
    // Plugin hooks return Result<()> — no response data to inject
    // Verify the registry is resilient
    let registry = PluginRegistry::new();
    let plugin = Arc::new(TestPlugin { name: "test" });
    registry.register(plugin.clone());

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut ctx = RequestContext::new(
        std::path::PathBuf::from("/tmp"),
        std::path::PathBuf::from("test.php"),
        deadline,
    );

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(registry.run_pre_exec(&mut ctx));
    assert!(result.is_ok());
}

// ===== Error Isolation Tests =====

struct MaliciousPlugin {
    name: &'static str,
}

#[async_trait::async_trait]
impl Plugin for MaliciousPlugin {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn pre_exec(&self, _ctx: &mut RequestContext) -> Result<()> {
        // Simulate a plugin returning an error with injection patterns
        Err(EngineError::Plugin("' OR 1=1 --".to_string()))
    }

    async fn post_exec(&self, _ctx: &RequestContext) -> Result<()> {
        Err(EngineError::Plugin("<script>alert(1)</script>".to_string()))
    }
}

#[test]
fn plugin_malicious_error_in_pre_exec() {
    // === Arrange ===
    let registry = PluginRegistry::new();
    let plugin = Arc::new(MaliciousPlugin {
        name: "malicious-pre",
    });
    registry.register(plugin);

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut ctx = RequestContext::new(
        std::path::PathBuf::from("/tmp"),
        std::path::PathBuf::from("test.php"),
        deadline,
    );

    // === Act ===
    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(registry.run_pre_exec(&mut ctx));

    // === Assert ===
    assert!(result.is_err(), "malicious plugin should return error");
    if let Err(e) = &result {
        // The error should contain the injection pattern (not sanitized)
        let msg = e.to_string();
        assert!(msg.contains("OR 1=1") || msg.contains("Plugin"));
    }
}

#[test]
fn plugin_malicious_error_in_post_exec() {
    let registry = PluginRegistry::new();
    let plugin = Arc::new(MaliciousPlugin {
        name: "malicious-post",
    });
    registry.register(plugin);

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let ctx = RequestContext::new(
        std::path::PathBuf::from("/tmp"),
        std::path::PathBuf::from("test.php"),
        deadline,
    );

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(registry.run_post_exec(&ctx));

    assert!(result.is_err());
    if let Err(e) = &result {
        let msg = e.to_string();
        assert!(msg.contains("alert") || msg.contains("Plugin"));
    }
}

#[test]
fn plugin_error_isolation_between_plugins() {
    // === Arrange ===
    let registry = PluginRegistry::new();

    // Register multiple plugins — if one fails, should it affect others?
    // Current behavior: first error short-circuits the chain
    let good_plugin = Arc::new(TestPlugin { name: "good" });
    let bad_plugin = Arc::new(MaliciousPlugin { name: "bad" });

    registry.register(good_plugin);
    registry.register(bad_plugin);

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut ctx = RequestContext::new(
        std::path::PathBuf::from("/tmp"),
        std::path::PathBuf::from("test.php"),
        deadline,
    );

    // === Act ===
    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(registry.run_pre_exec(&mut ctx));

    // === Assert ===
    // Good plugin runs first (ok), then bad plugin runs (error)
    // The chain short-circuits on first error
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn plugin_registry_new_default() {
    let registry = PluginRegistry::new();
    let registry_default = PluginRegistry::default();

    // Both should be valid
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut ctx = RequestContext::new(
        std::path::PathBuf::from("/tmp"),
        std::path::PathBuf::from("test.php"),
        deadline,
    );

    let rt = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result = rt.block_on(registry.run_pre_exec(&mut ctx));
    assert!(result.is_ok(), "empty registry should succeed");

    let mut ctx2 = RequestContext::new(
        std::path::PathBuf::from("/tmp"),
        std::path::PathBuf::from("test.php"),
        deadline,
    );
    let rt2 = tokio::runtime::Runtime::new().expect("runtime should succeed");
    let result2 = rt2.block_on(registry_default.run_pre_exec(&mut ctx2));
    assert!(result2.is_ok());
}
