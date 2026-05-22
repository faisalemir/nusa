//! Per-tenant VFS abstraction for file system isolation.
//!
//! Skills applied:
//! - `m09-domain`: Tenant isolation via path resolution
//! - `m05-type-driven`: TenantId enforces type-level scoping
//! - `m15-anti-pattern`: Path traversal prevention without requiring paths to exist

use std::path::PathBuf;

use crate::types::TenantId;

/// Trait for tenant-aware virtual filesystem resolution.
pub trait TenantVfs: Send + Sync {
    /// Resolve a relative path within a tenant's VFS root.
    fn resolve_path(&self, tenant_id: &TenantId, relative: &str) -> Option<PathBuf>;

    /// Get the VFS root for a tenant.
    fn vfs_root(&self, tenant_id: &TenantId) -> Option<PathBuf>;
}

/// Default VFS implementation: each tenant gets a subdirectory under a base path.
///
/// Path resolution: `{base_path}/{tenant_id}/{relative}`
/// m15-anti-pattern: Blocks `..`, absolute paths, and null bytes without requiring paths to exist on disk.
#[derive(Debug, Clone)]
pub struct DefaultTenantVfs {
    base_path: PathBuf,
}

impl DefaultTenantVfs {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    /// Check if a relative path component is safe (no traversal).
    fn is_safe_relative(relative: &str) -> bool {
        // Reject empty paths that aren't meaningful
        // Allow "." as current dir reference
        if relative.is_empty() || relative == "." {
            return true;
        }

        // Reject null bytes and percent-encoded traversal attempts
        if relative.contains('\0') || relative.contains('%') {
            return false;
        }

        // Reject POSIX absolute paths and Windows drive-letter paths (e.g. C:\ or C:/)
        if relative.starts_with('/') || relative.starts_with('\\') {
            return false;
        }
        if let Some((drive, rest)) = relative.split_once(':')
            && drive.len() == 1
            && drive.as_bytes()[0].is_ascii_alphabetic()
            && (rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\'))
        {
            return false;
        }

        // Reject .. components
        for component in relative.split(['/', '\\']) {
            if component == ".." {
                return false;
            }
        }

        true
    }
}

impl TenantVfs for DefaultTenantVfs {
    fn resolve_path(&self, tenant_id: &TenantId, relative: &str) -> Option<PathBuf> {
        // m15-anti-pattern: Validate before joining
        if !Self::is_safe_relative(relative) {
            return None;
        }

        // Build the resolved path
        let mut resolved = self.base_path.join(tenant_id.as_str());
        if !relative.is_empty() && relative != "." {
            resolved = resolved.join(relative);
        }

        // Additional check: try canonicalize if path exists, otherwise trust our validation
        if let Ok(canonical) = resolved.canonicalize() {
            let tenant_root = self
                .base_path
                .join(tenant_id.as_str())
                .canonicalize()
                .ok()?;
            if canonical.starts_with(&tenant_root) {
                Some(canonical)
            } else {
                None // Escape detected despite our validation
            }
        } else {
            // Path doesn't exist yet — return the logically resolved path
            // Our is_safe_relative check prevents traversal
            Some(resolved)
        }
    }

    fn vfs_root(&self, tenant_id: &TenantId) -> Option<PathBuf> {
        let path = self.base_path.join(tenant_id.as_str());
        if path.exists() { Some(path) } else { None }
    }
}
