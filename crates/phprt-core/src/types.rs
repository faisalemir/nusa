//! Newtype wrappers and request/response types.
//!
//! Skills applied:
//! - `m05-type-driven`: Newtype wrappers (TraceId, TenantId, WorkerId)
//! - `m09-domain`: RequestContext as aggregate root, immutable after construction
//! - `coding-guidelines`: No get_ prefix on accessors

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use bytes::Bytes;
use http::HeaderMap;
use uuid::Uuid;

/// Newtype wrappers to prevent context mixing (m05-type-driven)
/// Inner fields are private — encapsulation enforced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TraceId(Uuid);

impl TraceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for TraceId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TraceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TenantId(String);

impl TenantId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct WorkerId(usize);

impl WorkerId {
    pub fn new(id: usize) -> Self {
        Self(id)
    }

    pub fn as_usize(&self) -> usize {
        self.0
    }
}

impl std::fmt::Display for WorkerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "worker-{}", self.0)
    }
}

/// Immutable Request Context after validation (m09-domain)
/// Fields are private — mutation only via builder methods.
#[derive(Clone)]
pub struct RequestContext {
    trace_id: TraceId,
    tenant_id: Option<TenantId>,
    deadline: tokio::time::Instant,
    env: Arc<HashMap<String, String>>,
    vfs_root: PathBuf,
    script_path: PathBuf,
    body: Bytes,
    headers: HeaderMap,
}

impl RequestContext {
    #[must_use]
    pub fn new(
        vfs_root: PathBuf,
        script_path: PathBuf,
        deadline: tokio::time::Instant,
    ) -> Self {
        Self {
            trace_id: TraceId::new(),
            tenant_id: None,
            deadline,
            env: Arc::new(HashMap::new()),
            vfs_root,
            script_path,
            body: Bytes::new(),
            headers: HeaderMap::new(),
        }
    }

    #[must_use]
    pub fn with_tenant(mut self, tenant_id: TenantId) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }

    #[must_use]
    pub fn with_body(mut self, body: Bytes) -> Self {
        self.body = body;
        self
    }

    #[must_use]
    pub fn with_headers(mut self, headers: HeaderMap) -> Self {
        self.headers = headers;
        self
    }

    #[must_use]
    pub fn with_env(mut self, env: Arc<HashMap<String, String>>) -> Self {
        self.env = env;
        self
    }

    // Read-only accessors
    pub fn trace_id(&self) -> TraceId {
        self.trace_id
    }

    pub fn tenant_id(&self) -> Option<&TenantId> {
        self.tenant_id.as_ref()
    }

    pub fn deadline(&self) -> tokio::time::Instant {
        self.deadline
    }

    pub fn env(&self) -> &Arc<HashMap<String, String>> {
        &self.env
    }

    pub fn vfs_root(&self) -> &PathBuf {
        &self.vfs_root
    }

    pub fn script_path(&self) -> &PathBuf {
        &self.script_path
    }

    pub fn body(&self) -> &Bytes {
        &self.body
    }

    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }
}

/// Standardized PHP Response
#[derive(Debug, Clone)]
pub struct PhpResponse {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl PhpResponse {
    /// Create a response with the given status and body.
    ///
    /// # Example
    /// ```rust
    /// use phprt_core::PhpResponse;
    /// use http::HeaderMap;
    ///
    /// let resp = PhpResponse::ok(200, b"Hello World".to_vec());
    /// assert_eq!(resp.status, 200);
    /// assert_eq!(resp.body, bytes::Bytes::from("Hello World"));
    /// ```
    pub fn ok(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: Bytes::from(body),
        }
    }

    /// Create an error response.
    pub fn error(status: u16, message: impl Into<Bytes>) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: message.into(),
        }
    }
}
