//! Laravel plug-and-play config: env-only load, path normalization, resolution.

use nusa_config::{ConfigResolution, get, load_sources, resolve_config_path};
use serial_test::serial;

#[serial]
#[test]
fn env_only_load_without_toml_file() {
    let dir = std::env::temp_dir().join("nusa_env_only_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let prev = std::env::current_dir().unwrap();
    std::env::set_current_dir(&dir).unwrap();

    unsafe {
        std::env::set_var("NUSA_CODE_DIR", "/app");
        std::env::set_var("NUSA_OCTANE_WORKERS", "2");
        std::env::set_var("NUSA_BIND", "127.0.0.1:19999");
    }

    load_sources(None).expect("env-only load");

    let cfg = get();
    assert_eq!(cfg.code_dir, "/app");
    assert_eq!(cfg.octane_workers, 2);
    assert_eq!(cfg.bind, "127.0.0.1:19999");

    unsafe {
        std::env::remove_var("NUSA_CODE_DIR");
        std::env::remove_var("NUSA_OCTANE_WORKERS");
        std::env::remove_var("NUSA_BIND");
    }
    std::env::set_current_dir(prev).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[serial]
#[test]
fn resolve_config_env_only_when_default_missing() {
    let dir = std::env::temp_dir().join("nusa_resolve_none");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let prev = std::env::current_dir().unwrap();
    std::env::set_current_dir(&dir).unwrap();

    let resolution = resolve_config_path("nusa.toml").expect("resolve");
    assert_eq!(resolution, ConfigResolution::EnvOnly);

    std::env::set_current_dir(prev).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[serial]
#[test]
fn normalize_sets_vfs_root_from_artisan_root() {
    let dir = std::env::temp_dir().join("nusa_norm_paths");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("public")).unwrap();
    std::fs::write(dir.join("artisan"), "stub").unwrap();

    let file = dir.join("nusa.toml");
    let code_dir = dir.to_string_lossy().replace('\\', "/");
    std::fs::write(
        &file,
        format!(
            r#"
engine = "child"
max_workers = 2
code_dir = "{code_dir}"
vfs_root = ""
tmp_dir = "/tmp/nusa-test"
"#
        ),
    )
    .unwrap();

    load_sources(Some(file.to_str().unwrap())).expect("load");

    let cfg = get();
    assert_eq!(
        std::path::Path::new(&cfg.code_dir),
        dir.as_path(),
        "code_dir should match project root"
    );
    assert_eq!(
        std::path::Path::new(&cfg.vfs_root),
        dir.join("public").as_path(),
        "vfs_root should be public/"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
