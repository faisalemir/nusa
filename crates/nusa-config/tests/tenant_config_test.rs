//! Tenant registry entries from TOML config.

use nusa_config::{TenantEntry, load_sources};
use serial_test::serial;

#[serial]
#[test]
fn tenants_load_from_toml() {
    let dir = std::env::temp_dir().join("nusa_tenant_cfg");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("nusa.toml");
    std::fs::write(
        &file,
        r#"
engine = "child"
max_workers = 2
code_dir = "/app"
vfs_root = "/app/public"
tmp_dir = "/tmp/nusa"
[[tenants]]
id = "acme"
vfs_root = "/tenants/acme/public"
enabled = true
"#,
    )
    .unwrap();

    load_sources(Some(file.to_str().unwrap())).expect("load");
    let cfg = nusa_config::get();
    assert_eq!(cfg.tenants.len(), 1);
    assert_eq!(cfg.tenants[0].id, "acme");
    assert_eq!(cfg.tenants[0].vfs_root, "/tenants/acme/public");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tenant_entry_default_limits() {
    let entry = TenantEntry {
        id: "x".into(),
        vfs_root: "/p".into(),
        enabled: true,
        max_memory_mb: 512,
        max_requests_per_minute: 1000,
    };
    assert!(entry.enabled);
    assert_eq!(entry.max_memory_mb, 512);
}
