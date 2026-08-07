//! Static file handler with LRU cache and range request support.
//! Blueprint 6 E1 / P4-E: High-performance static file serving (Tier-S1).
//!
//! Skills applied:
//! - `domain-web`: HTTP static file serving, MIME type detection, Cache-Control headers
//! - `m10-performance`: moka LRU cache for small files <1MB, async tokio::fs reads
//! - `m15-anti-pattern`: Directory traversal prevention via canonicalization

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{Method, StatusCode, header},
    response::Response,
};
use moka::future::Cache;

/// Cache-Control tuning for static assets (from `nusa.toml`).
#[derive(Clone, Copy, Debug)]
pub struct StaticServeConfig {
    pub default_max_age_secs: u64,
    pub immutable_max_age_secs: u64,
    pub cache_max_entries: u64,
    pub cache_ttl_secs: u64,
}

impl Default for StaticServeConfig {
    fn default() -> Self {
        Self {
            default_max_age_secs: 3600,
            immutable_max_age_secs: 31_536_000,
            cache_max_entries: 1000,
            cache_ttl_secs: 300,
        }
    }
}

impl StaticServeConfig {
    #[must_use]
    pub fn from_runtime(cfg: &nusa_config::RuntimeConfig) -> Self {
        Self {
            default_max_age_secs: cfg.static_cache_max_age_secs,
            immutable_max_age_secs: cfg.static_cache_immutable_max_age_secs,
            cache_max_entries: cfg.static_cache_max_entries.max(1),
            cache_ttl_secs: cfg.static_cache_ttl_secs.max(1),
        }
    }
}

/// Static file cache entry.
#[derive(Clone)]
struct CachedFile {
    content: Vec<u8>,
    content_type: String,
    cache_control: String,
    content_encoding: Option<&'static str>,
}

/// Static file handler with LRU cache.
///
/// m10-performance: moka async cache with TTL prevents repeated disk reads for popular assets.
/// m15-anti-pattern: sanitize_path() blocks directory traversal attacks.
pub struct StaticFileHandler {
    public_dir: PathBuf,
    cache: Cache<String, CachedFile>,
    serve_config: StaticServeConfig,
}

impl StaticFileHandler {
    #[must_use]
    pub fn new(public_dir: PathBuf) -> Self {
        Self::with_config(public_dir, StaticServeConfig::default())
    }

    #[must_use]
    pub fn with_config(public_dir: PathBuf, serve_config: StaticServeConfig) -> Self {
        Self {
            public_dir,
            cache: Cache::builder()
                .max_capacity(serve_config.cache_max_entries)
                .time_to_live(std::time::Duration::from_secs(serve_config.cache_ttl_secs))
                .build(),
            serve_config,
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

    /// Serve a static file with proper headers and cache (GET or HEAD).
    ///
    /// When `Accept-Encoding` includes `br` or `gzip`, serves sibling `path.br` / `path.gz`
    /// if present (build-time precompression). Brotli wins when both are accepted.
    pub async fn serve(
        &self,
        path: &str,
        method: &Method,
        accept_encoding: Option<&str>,
    ) -> Option<Response<Body>> {
        let base_path = self.sanitize_path(path)?;
        let disk_path = self.select_variant(&base_path, accept_encoding)?;

        if !disk_path.is_file() {
            return None;
        }

        let cache_key = format!(
            "{}:{}",
            disk_path.to_string_lossy(),
            accept_encoding.unwrap_or("-")
        );

        let cached = if let Some(hit) = self.cache.get(&cache_key).await {
            hit
        } else {
            let content = tokio::fs::read(&disk_path).await.ok()?;
            let logical = strip_precompressed_suffix(&disk_path);
            let content_type = Self::content_type(&logical);
            let cache_control = self.cache_control(&logical);
            let content_encoding = encoding_for_path(&disk_path);
            let entry = CachedFile {
                content,
                content_type,
                cache_control,
                content_encoding,
            };
            if entry.content.len() < 1024 * 1024 {
                self.cache.insert(cache_key, entry.clone()).await;
            }
            entry
        };

        Some(self.build_response(cached, *method == Method::HEAD))
    }

    /// Sanitize path to prevent directory traversal (m15-anti-pattern).
    fn sanitize_path(&self, path: &str) -> Option<PathBuf> {
        let full_path = self.public_dir.join(path.trim_start_matches('/'));
        let canonical = full_path.canonicalize().ok()?;
        let public_canonical = self.public_dir.canonicalize().ok()?;

        if canonical.starts_with(&public_canonical) {
            Some(canonical)
        } else {
            None
        }
    }

    /// Pick on-disk file: `.br` > `.gz` > raw when client accepts encodings.
    fn select_variant(&self, base: &Path, accept_encoding: Option<&str>) -> Option<PathBuf> {
        if client_accepts_encoding(accept_encoding, "br") {
            let br = path_with_suffix(base, ".br");
            if br.is_file() {
                return Some(br);
            }
        }
        if client_accepts_encoding(accept_encoding, "gzip") {
            let gz = path_with_suffix(base, ".gz");
            if gz.is_file() {
                return Some(gz);
            }
        }
        base.is_file().then(|| base.to_path_buf())
    }

    fn build_response(&self, cached: CachedFile, head_only: bool) -> Response<Body> {
        let content_len = cached.content.len();
        let body = if head_only {
            Body::empty()
        } else {
            Body::from(cached.content)
        };
        let mut response = Response::new(body);
        *response.status_mut() = StatusCode::OK;
        let headers = response.headers_mut();
        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_str(&cached.content_type).unwrap(),
        );
        headers.insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_str(&cached.cache_control).unwrap(),
        );
        headers.insert(
            header::CONTENT_LENGTH,
            header::HeaderValue::from(content_len),
        );
        if let Some(enc) = cached.content_encoding {
            headers.insert(
                header::CONTENT_ENCODING,
                header::HeaderValue::from_static(enc),
            );
        }
        headers.insert(
            header::HeaderName::from_static("x-nusa-tier"),
            header::HeaderValue::from_static("S1"),
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

    fn cache_control(&self, path: &Path) -> String {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

        if stem.contains('.') {
            format!(
                "public, max-age={}, immutable",
                self.serve_config.immutable_max_age_secs
            )
        } else {
            format!("public, max-age={}", self.serve_config.default_max_age_secs)
        }
    }
}

fn path_with_suffix(base: &Path, suffix: &str) -> PathBuf {
    let mut os = base.as_os_str().to_os_string();
    os.push(suffix);
    PathBuf::from(os)
}

fn strip_precompressed_suffix(path: &Path) -> PathBuf {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return path.to_path_buf();
    };
    let stripped = name
        .strip_suffix(".br")
        .or_else(|| name.strip_suffix(".gz"))
        .unwrap_or(name);
    let mut logical = path.to_path_buf();
    logical.set_file_name(stripped);
    logical
}

fn encoding_for_path(path: &Path) -> Option<&'static str> {
    path.file_name().and_then(|s| s.to_str()).and_then(|name| {
        if name.ends_with(".br") {
            Some("br")
        } else if name.ends_with(".gz") {
            Some("gzip")
        } else {
            None
        }
    })
}

fn client_accepts_encoding(accept: Option<&str>, encoding: &str) -> bool {
    let Some(raw) = accept else {
        return false;
    };
    raw.split(',').any(|part| {
        part.split(';')
            .next()
            .map(|token| token.trim().eq_ignore_ascii_case(encoding))
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_br_and_gzip_tokens() {
        assert!(client_accepts_encoding(Some("gzip, deflate, br"), "br"));
        assert!(client_accepts_encoding(Some("gzip"), "gzip"));
        assert!(!client_accepts_encoding(Some("identity"), "br"));
    }

    #[test]
    fn encoding_for_precompressed_paths() {
        assert_eq!(encoding_for_path(Path::new("app.js.br")), Some("br"));
        assert_eq!(encoding_for_path(Path::new("app.js.gz")), Some("gzip"));
        assert_eq!(encoding_for_path(Path::new("app.js")), None);
    }
}
