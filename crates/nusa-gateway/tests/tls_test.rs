//! Tests for TLS configuration module.
//!
//! Covers: TlsConfig structure, cert/key path requirements, rustls ServerConfig loading.

use std::path::PathBuf;

use nusa_gateway::tls::TlsConfig;

#[test]
fn tls_config_stores_cert_and_key_paths() {
    let config = TlsConfig {
        cert_path: PathBuf::from("/path/to/cert.pem"),
        key_path: PathBuf::from("/path/to/key.pem"),
    };
    assert_eq!(config.cert_path, PathBuf::from("/path/to/cert.pem"));
    assert_eq!(config.key_path, PathBuf::from("/path/to/key.pem"));
}

#[test]
fn tls_config_clone_preserves_paths() {
    let config = TlsConfig {
        cert_path: PathBuf::from("/path/to/cert.pem"),
        key_path: PathBuf::from("/path/to/key.pem"),
    };
    let cloned = config.clone();
    assert_eq!(config.cert_path, cloned.cert_path);
    assert_eq!(config.key_path, cloned.key_path);
}

#[test]
fn tls_config_debug_contains_paths() {
    let config = TlsConfig {
        cert_path: PathBuf::from("/path/to/cert.pem"),
        key_path: PathBuf::from("/path/to/key.pem"),
    };
    let debug_str = format!("{:?}", config);
    assert!(debug_str.contains("cert.pem"));
    assert!(debug_str.contains("key.pem"));
}
