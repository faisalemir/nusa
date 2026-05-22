//! Helpers for live Nusa E2E tests (P2 Laravel fixture).

use std::path::{Path, PathBuf};

/// Resolve the Laravel minimal fixture directory.
///
/// Order: `NUSA_LARAVEL_FIXTURE` env → `tests/fixtures/laravel-minimal` relative to repo root.
pub fn laravel_fixture_root() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("NUSA_LARAVEL_FIXTURE") {
        let p = PathBuf::from(path);
        return fixture_ready(&p).then_some(p);
    }

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest.join("../../tests/fixtures/laravel-minimal"),
        manifest.join("../../../tests/fixtures/laravel-minimal"),
    ];

    for candidate in candidates {
        if fixture_ready(&candidate) {
            return candidate.canonicalize().ok();
        }
    }

    None
}

/// Returns true when `vendor/autoload.php`, `bootstrap/app.php`, and `php-driver` worker exist.
pub fn fixture_ready(path: &Path) -> bool {
    path.join("vendor/autoload.php").is_file()
        && path.join("bootstrap/app.php").is_file()
        && path.join("php-driver/bin/octane-rust-worker").is_file()
}

/// Panics with an actionable message when the fixture is not prepared.
pub fn require_laravel_fixture() -> PathBuf {
    laravel_fixture_root().unwrap_or_else(|| {
        panic!(
            "Laravel fixture not ready.\n\
             Alpine: run `just podman-build` (installs vendor in the image).\n\
             Linux host: run tests/fixtures/laravel-minimal/setup-fixture.sh"
        )
    })
}
