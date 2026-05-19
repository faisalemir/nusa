//! TLS 1.3 configuration for the gateway.
//!
//! Skills applied:
//! - `domain-cloud-native`: TLS termination, certificate management
//! - `m11-ecosystem`: rustls for safe TLS (no OpenSSL)

use std::path::PathBuf;

use rustls::ServerConfig;
use rustls_pemfile::{certs, private_key};
use std::fs::File;
use std::io::BufReader;

/// TLS configuration for HTTPS.
#[derive(Debug, Clone)]
pub struct TlsConfig {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}

impl TlsConfig {
    /// Load TLS certificates and create a rustls ServerConfig.
    pub fn load(&self) -> anyhow::Result<ServerConfig> {
        // Load certificate
        let cert_file = File::open(&self.cert_path)?;
        let mut cert_reader = BufReader::new(cert_file);
        let certs = certs(&mut cert_reader).collect::<Result<Vec<_>, _>>()?;

        // Load private key
        let key_file = File::open(&self.key_path)?;
        let mut key_reader = BufReader::new(key_file);
        let key = private_key(&mut key_reader)?.ok_or_else(|| {
            anyhow::anyhow!("No private key found in {}", self.key_path.display())
        })?;

        // Create TLS 1.3 server config
        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)?;

        Ok(config)
    }
}
