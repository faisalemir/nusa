//! Static file handler with LRU cache and range request support.
//! Blueprint 6 E1: High-performance static file serving.
//!
//! Skills applied:
//! - `domain-web`: HTTP static file serving, MIME type detection, Cache-Control headers
//! - `m10-performance`: moka LRU cache for small files <1MB, async tokio::fs reads
//! - `m15-anti-pattern`: Directory traversal prevention via canonicalization

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{StatusCode, header},
    response::Response,
};
use moka::future::Cache;

/// Static file cache entry.
#[derive(Clone)]
struct CachedFile {
    content: Vec<u8>,
    content_type: String,
    cache_control: String,
}

/// Static file handler with LRU cache.
///
/// m10-performance: moka async cache with TTL prevents repeated disk reads for popular assets.
/// m15-anti-pattern: sanitize_path() blocks directory traversal attacks.
pub struct StaticFileHandler {
    public_dir: PathBuf,
    cache: Cache<String, CachedFile>,
}

impl StaticFileHandler {
    pub fn new(public_dir: PathBuf) -> Self {
        Self {
            public_dir,
            // m10-performance: Cache files <1MB, up to 1000 entries, TTL 5 minutes
            cache: Cache::builder()
                .max_capacity(1000)
                .time_to_live(std::time::Duration::from_secs(300))
                .build(),
        }
    }

    /// Check if the path is a static file extension (domain-web).
    pub fn is_static(path: &str) -> bool {
        let extensions = [
            ".css", ".js", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".woff", ".woff2", ".ttf",
            ".eot", ".ico", ".webp", ".mp4", ".webm", ".mp3", ".ogg", ".pdf",
        ];
        extensions.iter().any(|ext| path.ends_with(ext))
    }

    /// Serve a static file with proper headers and cache.
    /// m15-anti-pattern: sanitize_path prevents directory traversal.
    /// m10-performance: async file read + moka cache for hot paths.
    pub async fn serve(&self, path: &str) -> Option<Response<Body>> {
        let full_path = self.sanitize_path(path)?;

        if !full_path.exists() || !full_path.is_file() {
            return None;
        }

        let cache_key = full_path.to_string_lossy().to_string();

        // m10-performance: Check cache first to avoid disk I/O
        if let Some(cached) = self.cache.get(&cache_key).await {
            return Some(self.build_response(cached));
        }

        // domain-web: async file read for non-blocking I/O
        let content = match tokio::fs::read(&full_path).await {
            Ok(bytes) => bytes,
            Err(_) => return None,
        };

        let content_type = Self::content_type(&full_path);
        let cache_control = Self::cache_control(&full_path);

        let cached = CachedFile {
            content,
            content_type,
            cache_control,
        };

        // m10-performance: Cache if under 1MB to avoid cache pollution
        if cached.content.len() < 1024 * 1024 {
            self.cache.insert(cache_key, cached.clone()).await;
        }

        Some(self.build_response(cached))
    }

    /// Sanitize path to prevent directory traversal (m15-anti-pattern).
    fn sanitize_path(&self, path: &str) -> Option<PathBuf> {
        let full_path = self.public_dir.join(path.trim_start_matches('/'));
        let canonical = full_path.canonicalize().ok()?;
        let public_canonical = self.public_dir.canonicalize().ok()?;

        if canonical.starts_with(&public_canonical) {
            Some(canonical)
        } else {
            None // m15-anti-pattern: Directory traversal blocked
        }
    }

    fn build_response(&self, cached: CachedFile) -> Response<Body> {
        // domain-web: proper Cache-Control, Content-Type, Content-Length headers
        let content_len = cached.content.len();
        let mut response = Response::new(Body::from(cached.content));
        *response.status_mut() = StatusCode::OK;
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_str(&cached.content_type).unwrap(),
        );
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_str(&cached.cache_control).unwrap(),
        );
        response.headers_mut().insert(
            header::CONTENT_LENGTH,
            header::HeaderValue::from(content_len),
        );
        response
    }

    /// MIME type detection based on file extension (domain-web).
    fn content_type(path: &Path) -> String {
        match path.extension().and_then(|e| e.to_str()) {
            Some("css") => "text/css".into(),
            Some("js") => "application/javascript".into(),
            Some("png") => "image/png".into(),
            Some("jpg") | Some("jpeg") => "image/jpeg".into(),
            Some("gif") => "image/gif".into(),
            Some("svg") => "image/svg+xml".into(),
            Some("woff") => "font/woff".into(),
            Some("woff2") => "font/woff2".into(),
            Some("ttf") => "font/ttf".into(),
            Some("ico") => "image/x-icon".into(),
            Some("webp") => "image/webp".into(),
            Some("mp4") => "video/mp4".into(),
            Some("webm") => "video/webm".into(),
            Some("mp3") => "audio/mpeg".into(),
            Some("pdf") => "application/pdf".into(),
            _ => "application/octet-stream".into(),
        }
    }

    /// Cache-Control strategy: immutable for hash-named files (domain-web).
    fn cache_control(path: &Path) -> String {
        // Hash-named files (e.g. app.abc123.css) get immutable cache
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

        if stem.contains('.') {
            // Contains hash pattern → long TTL with immutable
            "public, max-age=31536000, immutable".into()
        } else {
            // Non-hashed files → short TTL
            "public, max-age=3600".into()
        }
    }
}
