//! TLS 1.3 configuration for the gateway.
//!
//! Skills applied:
//! - `domain-cloud-native`: TLS termination, certificate management
//! - `m11-ecosystem`: rustls for safe TLS (no OpenSSL)

use std::path::PathBuf;

use rustls::ServerConfig;
use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};

/// TLS configuration for HTTPS.
#[derive(Debug, Clone)]
pub struct TlsConfig {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}

impl TlsConfig {
    /// Load TLS certificates and create a rustls ServerConfig.
    pub fn load(&self) -> anyhow::Result<ServerConfig> {
        // Load certificates
        let certs =
            CertificateDer::pem_file_iter(&self.cert_path)?.collect::<Result<Vec<_>, _>>()?;

        // Load private key
        let key = PrivateKeyDer::from_pem_file(&self.key_path)?;

        // Create TLS 1.3 server config
        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)?;

        Ok(config)
    }
}
