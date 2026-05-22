//! Build script for generating FFI bindings to libphp (ZTS).
//!
//! Skills applied:
//! - `m11-ecosystem`: bindgen for FFI code generation
//! - `unsafe-checker`: Only generates code, no unsafe execution
//! - `coding-guidelines`: Environment variable defaults with unwrap_or_else

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-env-changed=PHP_INCLUDE_DIR");

    // Register cfg flags so clippy doesn't complain
    println!("cargo::rustc-check-cfg=cfg(php_embed_available)");

    // Only attempt bindgen on Linux with embed headers available.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "linux" {
        return;
    }

    // musl build scripts cannot dlopen libclang; stub unless PHP dev headers are wired in.
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let php_include = match std::env::var("PHP_INCLUDE_DIR") {
        Ok(dir) => dir,
        Err(_) if target_env == "musl" => {
            eprintln!(
                "Warning: skipping bindgen on musl without PHP_INCLUDE_DIR. Using stub implementation."
            );
            return;
        }
        Err(_) => "/usr/include/php".into(),
    };

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", php_include))
        .clang_arg(format!("-I{}/main", php_include))
        .clang_arg(format!("-I{}/Zend", php_include))
        .clang_arg(format!("-I{}/sapi/embed", php_include))
        .allowlist_function("php_embed_init")
        .allowlist_function("php_embed_shutdown")
        .allowlist_function("php_execute_script")
        .allowlist_function("php_request_shutdown")
        .allowlist_function("php_module_shutdown")
        .allowlist_var("php_embed_module")
        .allowlist_type("zend_file_handle")
        .allowlist_type("_sapi_module_struct")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate();

    match bindings {
        Ok(bindings) => {
            let out_path = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
            bindings
                .write_to_file(out_path.join("php_sys.rs"))
                .expect("failed to write bindings");
            println!("cargo::rustc-cfg=php_embed_available");
        }
        Err(e) => {
            // If bindgen fails (no PHP headers), use stub
            eprintln!("Warning: bindgen failed: {}. Using stub implementation.", e);
        }
    }
}
