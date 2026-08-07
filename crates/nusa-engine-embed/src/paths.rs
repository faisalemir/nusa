//! Resolve `php-driver` paths for embed workers (app-local symlink or repo checkout).

use std::path::{Path, PathBuf};

use crate::error::EmbedError;

/// Locate `nusa_embed_daemon.php` for the given Laravel `code_dir`.
pub fn resolve_embed_daemon(code_dir: &Path) -> Result<PathBuf, EmbedError> {
    if let Ok(root) = std::env::var("NUSA_PHP_DRIVER") {
        let daemon = PathBuf::from(root).join("embed/nusa_embed_daemon.php");
        if daemon.is_file() {
            return Ok(daemon);
        }
    }

    let in_app = code_dir.join("php-driver/embed/nusa_embed_daemon.php");
    if in_app.is_file() {
        return Ok(in_app);
    }

    let mut cur = code_dir.to_path_buf();
    for _ in 0..10 {
        let candidate = cur.join("php-driver/embed/nusa_embed_daemon.php");
        if candidate.is_file() {
            return Ok(candidate);
        }
        if !cur.pop() {
            break;
        }
    }

    Err(EmbedError::handshake(format!(
        "embed daemon missing (set NUSA_PHP_DRIVER or link php-driver into {}); expected php-driver/embed/nusa_embed_daemon.php",
        code_dir.display()
    )))
}

/// Directory containing `composer.json` for `nusa/octane` (php-driver package root).
pub fn resolve_php_driver_root(code_dir: &Path) -> Option<PathBuf> {
    if let Ok(root) = std::env::var("NUSA_PHP_DRIVER") {
        let path = PathBuf::from(root);
        if path.join("src/Embed/Runtime.php").is_file() {
            return Some(path);
        }
    }

    let in_app = code_dir.join("php-driver");
    if in_app.join("src/Embed/Runtime.php").is_file() {
        return Some(in_app);
    }

    let mut cur = code_dir.to_path_buf();
    for _ in 0..10 {
        let candidate = cur.join("php-driver");
        if candidate.join("src/Embed/Runtime.php").is_file() {
            return Some(candidate);
        }
        if !cur.pop() {
            break;
        }
    }

    None
}
