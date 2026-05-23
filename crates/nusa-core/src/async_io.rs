//! Async I/O offload bridge (P4-D spike). Default: disabled no-op.
//!
//! Future: PDO/HTTP proxies submit work to Rust async pools and park PHP workers.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;

/// Errors from the async I/O bridge (spike surface).
#[derive(Debug, Error)]
pub enum AsyncIoError {
    #[error("async I/O bridge disabled")]
    Disabled,
    #[error("operation not supported by this bridge")]
    NotSupported,
    #[error("read-only SQL required (SELECT/WITH/PRAGMA/EXPLAIN)")]
    DisallowedSql,
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("async worker join failed")]
    Join,
}

/// Result of a read-only SQL statement.
#[derive(Debug, Clone, PartialEq)]
pub enum AsyncSqlResult {
    Scalar(i64),
    Rows(Value),
}

/// Pluggable async I/O facade for Octane workers (not wired to Eloquent yet).
#[async_trait]
pub trait AsyncIoBridge: Send + Sync {
    fn name(&self) -> &'static str;

    /// When false, Laravel uses native blocking drivers only.
    async fn enabled(&self) -> bool {
        false
    }

    /// Spike round-trip: run `SELECT 1` on a blocking pool (proves Tokio offload path).
    async fn select_one(&self) -> Result<i64, AsyncIoError> {
        match self.execute_readonly_sql("SELECT 1").await? {
            AsyncSqlResult::Scalar(v) => Ok(v),
            AsyncSqlResult::Rows(rows) => first_scalar_from_rows(&rows),
        }
    }

    /// Execute a single read-only SQL statement on the async pool (P4-D spike).
    async fn execute_readonly_sql(&self, sql: &str) -> Result<AsyncSqlResult, AsyncIoError> {
        let _ = (self, sql);
        Err(AsyncIoError::NotSupported)
    }
}

/// Production default until P4-D lands.
pub struct NoopAsyncIoBridge;

#[async_trait]
impl AsyncIoBridge for NoopAsyncIoBridge {
    fn name(&self) -> &'static str {
        "noop"
    }
}

/// P4-D spike: SQLite read-only queries via `spawn_blocking`.
pub struct SpikeSqliteAsyncIoBridge {
    conn: Arc<parking_lot::Mutex<rusqlite::Connection>>,
}

impl SpikeSqliteAsyncIoBridge {
    /// Open a shared SQLite connection for the spike bridge.
    pub fn new() -> Result<Self, AsyncIoError> {
        let conn = open_spike_connection()?;
        Ok(Self {
            conn: Arc::new(parking_lot::Mutex::new(conn)),
        })
    }
}

impl Default for SpikeSqliteAsyncIoBridge {
    fn default() -> Self {
        Self::new().expect("spike SQLite connection failed")
    }
}

#[async_trait]
impl AsyncIoBridge for SpikeSqliteAsyncIoBridge {
    fn name(&self) -> &'static str {
        "spike-sqlite"
    }

    async fn enabled(&self) -> bool {
        true
    }

    async fn execute_readonly_sql(&self, sql: &str) -> Result<AsyncSqlResult, AsyncIoError> {
        if !is_readonly_sql(sql) {
            return Err(AsyncIoError::DisallowedSql);
        }
        let sql = sql.to_string();
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || run_readonly_sqlite(&conn, &sql))
            .await
            .map_err(|_| AsyncIoError::Join)?
    }
}

/// True for single-statement read-only SQL (spike allowlist).
pub fn is_readonly_sql(sql: &str) -> bool {
    let trimmed = sql.trim();
    if trimmed.is_empty() || trimmed.contains(';') {
        return false;
    }
    let upper = trimmed.to_ascii_uppercase();
    let first = upper.split_whitespace().next().unwrap_or("");
    matches!(first, "SELECT" | "WITH" | "PRAGMA" | "EXPLAIN")
        && !upper.contains(" INTO ")
        && !upper.contains(" FOR UPDATE")
}

fn open_spike_connection() -> Result<rusqlite::Connection, rusqlite::Error> {
    if let Ok(path) = std::env::var("NUSA_ASYNC_SQLITE_PATH")
        && !path.is_empty()
    {
        rusqlite::Connection::open(path)
    } else {
        rusqlite::Connection::open_in_memory()
    }
}

fn run_readonly_sqlite(
    conn: &parking_lot::Mutex<rusqlite::Connection>,
    sql: &str,
) -> Result<AsyncSqlResult, AsyncIoError> {
    let conn = conn.lock();
    let mut stmt = conn.prepare(sql)?;
    let col_count = stmt.column_count();
    if col_count == 0 {
        return Ok(AsyncSqlResult::Rows(Value::Array(vec![])));
    }
    let names: Vec<String> = (0..col_count)
        .map(|i| stmt.column_name(i).unwrap_or("").to_string())
        .collect();

    let rows: Vec<Value> = stmt
        .query_map([], |row| {
            let mut obj = serde_json::Map::new();
            for (i, name) in names.iter().enumerate() {
                let cell: rusqlite::types::Value = row.get(i)?;
                obj.insert(name.clone(), sqlite_value_to_json(cell));
            }
            Ok(Value::Object(obj))
        })?
        .collect::<Result<Vec<_>, rusqlite::Error>>()?;

    if rows.len() == 1
        && col_count == 1
        && let Some(Value::Object(obj)) = rows.first()
        && let Some(Value::Number(n)) = obj.values().next()
        && let Some(i) = n.as_i64()
    {
        return Ok(AsyncSqlResult::Scalar(i));
    }

    Ok(AsyncSqlResult::Rows(Value::Array(rows)))
}

fn first_scalar_from_rows(rows: &Value) -> Result<i64, AsyncIoError> {
    let Some(first) = rows.as_array().and_then(|a| a.first()) else {
        return Err(AsyncIoError::NotSupported);
    };
    let Some(n) = first
        .as_object()
        .and_then(|o| o.values().next())
        .and_then(|v| v.as_i64())
    else {
        return Err(AsyncIoError::NotSupported);
    };
    Ok(n)
}

fn sqlite_value_to_json(value: rusqlite::types::Value) -> Value {
    match value {
        rusqlite::types::Value::Null => Value::Null,
        rusqlite::types::Value::Integer(i) => Value::from(i),
        rusqlite::types::Value::Real(f) => {
            serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number)
        }
        rusqlite::types::Value::Text(s) => Value::String(s),
        rusqlite::types::Value::Blob(b) => Value::String(format!("<blob {} bytes>", b.len())),
    }
}

/// Resolve bridge from env.
///
/// | `NUSA_ASYNC_IO` | Bridge |
/// |-----------------|--------|
/// | `stub` | [`SpikeSqliteAsyncIoBridge`] |
/// | `1` / `true` | [`NoopAsyncIoBridge`] (reserved; logs warning at server start) |
/// | unset | noop |
pub fn bridge_from_env() -> Box<dyn AsyncIoBridge> {
    match std::env::var("NUSA_ASYNC_IO").ok().as_deref() {
        Some("stub") => match SpikeSqliteAsyncIoBridge::new() {
            Ok(bridge) => Box::new(bridge),
            Err(e) => {
                eprintln!("spike SQLite bridge failed to open connection, using noop: {e}");
                Box::new(NoopAsyncIoBridge)
            }
        },
        _ => Box::new(NoopAsyncIoBridge),
    }
}

/// True when `NUSA_ASYNC_IO` requests an experimental bridge.
pub fn async_io_requested_from_env() -> bool {
    matches!(
        std::env::var("NUSA_ASYNC_IO").ok().as_deref(),
        Some("1") | Some("true") | Some("stub")
    )
}

/// True when the stub SQLite bridge is active (`NUSA_ASYNC_IO=stub`).
pub fn async_io_stub_from_env() -> bool {
    std::env::var("NUSA_ASYNC_IO").ok().as_deref() == Some("stub")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readonly_sql_allowlist() {
        assert!(is_readonly_sql("SELECT 1"));
        assert!(is_readonly_sql("  select count(*) from users "));
        assert!(is_readonly_sql("WITH t AS (SELECT 1) SELECT * FROM t"));
        assert!(!is_readonly_sql("INSERT INTO x VALUES (1)"));
        assert!(!is_readonly_sql("SELECT 1; DROP TABLE x"));
    }

    #[tokio::test]
    async fn noop_select_one_not_supported() {
        let bridge = NoopAsyncIoBridge;
        assert!(!bridge.enabled().await);
        assert!(matches!(
            bridge.select_one().await,
            Err(AsyncIoError::NotSupported)
        ));
    }

    #[tokio::test]
    async fn spike_sqlite_select_one_roundtrip() {
        let bridge = SpikeSqliteAsyncIoBridge::default();
        assert!(bridge.enabled().await);
        assert_eq!(bridge.select_one().await.expect("select 1"), 1);
    }

    #[tokio::test]
    async fn spike_sqlite_readonly_rows() {
        let bridge = SpikeSqliteAsyncIoBridge::default();
        let result = bridge
            .execute_readonly_sql("SELECT 2 AS two, 'ok' AS label")
            .await
            .expect("query");
        let AsyncSqlResult::Rows(rows) = result else {
            panic!("expected rows");
        };
        let arr = rows.as_array().expect("array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["two"], 2);
        assert_eq!(arr[0]["label"], "ok");
    }
}
