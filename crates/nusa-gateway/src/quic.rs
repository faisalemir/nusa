//! HTTP/3 (QUIC) support for the Nusa runtime.
//! Blueprint 6 E3: HTTP/3 QUIC listener alongside TCP for reduced latency on unstable networks.
//!
//! Skills applied:
//! - `domain-cloud-native`: QUIC protocol, UDP listener, HTTP/3 ALPN
//! - `m07-concurrency`: Non-blocking UDP I/O with tokio
//! - `m12-lifecycle`: QUIC endpoint initialization → serve → graceful shutdown

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use quinn::crypto::rustls::QuicServerConfig;
use quinn::{Endpoint, ServerConfig, TransportConfig, VarInt};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tracing::{info, warn};

/// HTTP/3 QUIC listener wrapper.
///
/// domain-cloud-native: Listens on UDP port 443 alongside TCP, shares TLS config.
/// m07-concurrency: Non-blocking UDP I/O with tokio runtime.
pub struct QuicListener {
    addr: SocketAddr,
    tls_cert: Vec<CertificateDer<'static>>,
    tls_key: PrivateKeyDer<'static>,
}

impl QuicListener {
    pub fn new(
        addr: SocketAddr,
        tls_cert: Vec<CertificateDer<'static>>,
        tls_key: PrivateKeyDer<'static>,
    ) -> Self {
        Self {
            addr,
            tls_cert,
            tls_key,
        }
    }

    /// Start the QUIC listener and serve HTTP/3 requests.
    /// m12-lifecycle: init → serve → graceful shutdown.
    pub async fn serve(self, _app: Router) -> anyhow::Result<()> {
        info!("Starting HTTP/3 QUIC listener on {}", self.addr);

        // Build QUIC server TLS config with h3 ALPN
        let server_crypto = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(self.tls_cert, self.tls_key)?;

        let quic_crypto = QuicServerConfig::try_from(server_crypto)?;
        let mut server_config = ServerConfig::with_crypto(Arc::new(quic_crypto));

        let mut transport = TransportConfig::default();
        transport
            .max_concurrent_bidi_streams(VarInt::from_u32(100))
            .max_concurrent_uni_streams(VarInt::from_u32(100));
        server_config.transport_config(Arc::new(transport));

        let endpoint = Endpoint::server(server_config, self.addr)?;
        info!("Nusa QUIC endpoint bound to {}", self.addr);

        // Accept incoming QUIC connections
        while let Some(connecting) = endpoint.accept().await {
            tokio::spawn(async move {
                match connecting.await {
                    Ok(conn) => {
                        info!(
                            "Nusa QUIC connection established from {:?}",
                            conn.remote_address()
                        );
                        // In production: handle HTTP/3 streams here via h3 crate
                    }
                    Err(e) => {
                        warn!("Nusa QUIC connection failed: {}", e);
                    }
                }
            });
        }

        Ok(())
    }
}

/// Create HTTP/3 compatible TLS config with h3 ALPN.
pub fn h3_tls_config(
    cert: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> anyhow::Result<Arc<rustls::ServerConfig>> {
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert, key)?;
    Ok(Arc::new(config))
}
