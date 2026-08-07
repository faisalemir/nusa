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
    if !matches!(first, "SELECT" | "WITH" | "PRAGMA" | "EXPLAIN") {
        return false;
    }
    if upper.contains(" INTO ") || upper.contains(" FOR UPDATE") {
        return false;
    }
    first != "PRAGMA" || !upper.contains("WRITABLE")
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
#[allow(unsafe_code)] // env manipulation in tests requires unsafe due to crate-level deny
mod tests {
    use super::*;

    #[test]
    fn readonly_sql_allowlist_basic() {
        assert!(is_readonly_sql("SELECT 1"));
        assert!(is_readonly_sql("  select count(*) from users "));
        assert!(is_readonly_sql("WITH t AS (SELECT 1) SELECT * FROM t"));
        assert!(!is_readonly_sql("INSERT INTO x VALUES (1)"));
        assert!(!is_readonly_sql("SELECT 1; DROP TABLE x"));
    }

    // === S20: SQL Allowlist Bypass Matrix (15+ vectors) ===

    #[test]
    fn readonly_sql_allowlist_bypass_matrix() {
        // Stacked queries → false
        assert!(!is_readonly_sql("SELECT 1; DROP TABLE users"));

        // SELECT INTO → false
        assert!(!is_readonly_sql("SELECT * INTO OUTFILE '/tmp/x'"));

        // SELECT FOR UPDATE → false
        assert!(!is_readonly_sql("SELECT * FROM x FOR UPDATE"));

        // PRAGMA write → false
        assert!(!is_readonly_sql("PRAGMA writable_schema=1"));

        // PRAGMA read-only → true
        assert!(is_readonly_sql("PRAGMA journal_mode=WAL"));

        // Comment injection: no space between SELECT and /* → first word is "SELECT/*", rejected (safe default)
        assert!(!is_readonly_sql("SELECT/* comment */1"));

        // Comment with space before /* → first word is "SELECT", allowed
        assert!(is_readonly_sql("SELECT /* safe */ 1"));

        // Multiline injection → false (contains semicolon)
        assert!(!is_readonly_sql("SELECT\n1; DROP TABLE x"));

        // Lowercase normalized → true
        assert!(is_readonly_sql("select 1"));

        // Empty SQL → false
        assert!(!is_readonly_sql(""));

        // Whitespace-only → false
        assert!(!is_readonly_sql("   "));

        // EXPLAIN → true (read-only)
        assert!(is_readonly_sql("EXPLAIN SELECT * FROM users"));

        // Deeply nested SELECT → true (still read-only)
        assert!(is_readonly_sql("SELECT (SELECT (SELECT 1))"));

        // UPDATE → false
        assert!(!is_readonly_sql("UPDATE users SET name = 'evil'"));

        // DELETE → false
        assert!(!is_readonly_sql("DELETE FROM users WHERE 1=1"));

        // CREATE → false
        assert!(!is_readonly_sql("CREATE TABLE evil (id INT)"));

        // DROP → false
        assert!(!is_readonly_sql("DROP TABLE users"));

        // ALTER → false
        assert!(!is_readonly_sql("ALTER TABLE users ADD COLUMN evil INT"));

        // ATTACH → false (SQLite-specific write)
        assert!(!is_readonly_sql("ATTACH DATABASE 'evil.db' AS evil"));

        // DETACH → false
        assert!(!is_readonly_sql("DETACH DATABASE evil"));

        // REINDEX → false
        assert!(!is_readonly_sql("REINDEX"));

        // VACUUM → false
        assert!(!is_readonly_sql("VACUUM"));

        // BEGIN/COMMIT/ROLLBACK → false
        assert!(!is_readonly_sql("BEGIN TRANSACTION"));
        assert!(!is_readonly_sql("COMMIT"));
        assert!(!is_readonly_sql("ROLLBACK"));

        // PRAGMA with write intent → false (contains "writable")
        assert!(!is_readonly_sql("PRAGMA writable_schema = ON"));

        // CTE write body: WITH ... INSERT INTO — rejected because " INTO " is in the SQL
        let cte_write = "WITH x AS (SELECT 1) INSERT INTO y SELECT * FROM x";
        assert!(
            !is_readonly_sql(cte_write),
            "CTE with INSERT INTO must be rejected"
        );

        // SELECT with leading whitespace + newline → true
        assert!(is_readonly_sql("  \n  SELECT 1"));

        // Tab-separated → true
        assert!(is_readonly_sql("\tSELECT\t1"));

        // Single statement SELECT with trailing semicolon → false (semicolon check)
        assert!(!is_readonly_sql("SELECT 1;"));

        // Null byte in SQL → true (treated as regular char, no semicolon)
        assert!(is_readonly_sql("SELECT 1\x00"));
    }

    // === S20: bridge_from_env Env Tampering ===

    #[test]
    fn bridge_from_env_malicious_values_use_noop() {
        let malicious_values = [
            "DROP TABLE users",
            "../../etc/passwd",
            "evil",
            "1; DROP TABLE",
            "stub; rm -rf /",
            "",
            "SELECT 1",
            "PRAGMA writable_schema=1",
            "NUSA_ASYNC_SQLITE_PATH=../../etc/passwd",
        ];
        for value in malicious_values {
            unsafe { std::env::set_var("NUSA_ASYNC_IO", value) };
            let bridge = bridge_from_env();
            assert_eq!(
                bridge.name(),
                "noop",
                "NUSA_ASYNC_IO={value:?} must fall back to noop"
            );
        }
        unsafe { std::env::remove_var("NUSA_ASYNC_IO") };
    }

    #[test]
    fn bridge_from_env_reserved_values() {
        // "1" → noop (reserved for future)
        unsafe { std::env::set_var("NUSA_ASYNC_IO", "1") };
        assert_eq!(bridge_from_env().name(), "noop");

        // "true" → noop
        unsafe { std::env::set_var("NUSA_ASYNC_IO", "true") };
        assert_eq!(bridge_from_env().name(), "noop");

        // unset → noop
        unsafe { std::env::remove_var("NUSA_ASYNC_IO") };
        assert_eq!(bridge_from_env().name(), "noop");

        // "stub" → spike-sqlite
        unsafe { std::env::set_var("NUSA_ASYNC_IO", "stub") };
        let bridge = bridge_from_env();
        assert_eq!(bridge.name(), "spike-sqlite");
        unsafe { std::env::remove_var("NUSA_ASYNC_IO") };
    }

    // === S20: async_io_requested_from_env / async_io_stub_from_env ===

    #[test]
    fn async_io_requested_all_true_values() {
        for value in ["1", "true", "stub"] {
            unsafe { std::env::set_var("NUSA_ASYNC_IO", value) };
            assert!(
                async_io_requested_from_env(),
                "NUSA_ASYNC_IO={value} should request async I/O"
            );
        }
        unsafe { std::env::remove_var("NUSA_ASYNC_IO") };
    }

    #[test]
    fn async_io_requested_false_values() {
        for value in ["0", "false", "evil", "", "SELECT 1"] {
            unsafe { std::env::set_var("NUSA_ASYNC_IO", value) };
            assert!(
                !async_io_requested_from_env(),
                "NUSA_ASYNC_IO={value} should not request async I/O"
            );
        }
        unsafe { std::env::remove_var("NUSA_ASYNC_IO") };
    }

    #[test]
    fn async_io_stub_only_stub_value() {
        unsafe { std::env::set_var("NUSA_ASYNC_IO", "stub") };
        assert!(async_io_stub_from_env());
        unsafe { std::env::remove_var("NUSA_ASYNC_IO") };

        for value in ["1", "true", "evil", ""] {
            unsafe { std::env::set_var("NUSA_ASYNC_IO", value) };
            assert!(
                !async_io_stub_from_env(),
                "NUSA_ASYNC_IO={value} should not be stub"
            );
        }
        unsafe { std::env::remove_var("NUSA_ASYNC_IO") };
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

    // === S20: first_scalar_from_rows edge cases ===

    #[test]
    fn first_scalar_from_rows_empty_array() {
        let result = first_scalar_from_rows(&serde_json::Value::Array(vec![]));
        assert!(matches!(result, Err(AsyncIoError::NotSupported)));
    }

    #[test]
    fn first_scalar_from_rows_non_numeric() {
        let rows = serde_json::json!([{"name": "Alice"}]);
        let result = first_scalar_from_rows(&rows);
        assert!(matches!(result, Err(AsyncIoError::NotSupported)));
    }

    #[test]
    fn first_scalar_from_rows_valid_scalar() {
        let rows = serde_json::json!([{"count": 42}]);
        let result = first_scalar_from_rows(&rows);
        assert_eq!(result.expect("scalar"), 42);
    }

    // === S20: sqlite_value_to_json all variants ===

    #[test]
    fn sqlite_value_null() {
        let result = sqlite_value_to_json(rusqlite::types::Value::Null);
        assert!(result.is_null());
    }

    #[test]
    fn sqlite_value_integer() {
        let result = sqlite_value_to_json(rusqlite::types::Value::Integer(42));
        assert_eq!(result, serde_json::json!(42));
    }

    #[test]
    fn sqlite_value_real() {
        let result = sqlite_value_to_json(rusqlite::types::Value::Real(std::f64::consts::PI));
        let expected = serde_json::Number::from_f64(std::f64::consts::PI)
            .map_or(serde_json::Value::Null, serde_json::Value::Number);
        assert_eq!(result, expected);
    }

    #[test]
    fn sqlite_value_text() {
        let result = sqlite_value_to_json(rusqlite::types::Value::Text("hello".into()));
        assert_eq!(result, serde_json::json!("hello"));
    }

    #[test]
    fn sqlite_value_blob() {
        let blob = vec![0, 1, 2, 3];
        let result = sqlite_value_to_json(rusqlite::types::Value::Blob(blob));
        assert_eq!(result, serde_json::json!("<blob 4 bytes>"));
    }

    // === S20: SpikeSqliteAsyncIoBridge SQL rejection ===

    #[tokio::test]
    async fn spike_sqlite_rejects_write_sql() {
        let bridge = SpikeSqliteAsyncIoBridge::default();

        let write_queries = [
            "INSERT INTO x VALUES (1)",
            "UPDATE x SET y = 1",
            "DELETE FROM x",
            "DROP TABLE x",
            "CREATE TABLE x (id INT)",
            "SELECT 1; DROP TABLE x",
            "SELECT * INTO OUTFILE '/tmp/x'",
            "SELECT * FROM x FOR UPDATE",
            "PRAGMA writable_schema=1",
        ];
        for sql in write_queries {
            let result = bridge.execute_readonly_sql(sql).await;
            assert!(
                matches!(result, Err(AsyncIoError::DisallowedSql)),
                "{sql:?} must be rejected, got {result:?}"
            );
        }
    }

    #[tokio::test]
    async fn spike_sqlite_accepts_readonly_sql() {
        let bridge = SpikeSqliteAsyncIoBridge::default();

        let readonly_queries = [
            "SELECT 1",
            "SELECT count(*) FROM sqlite_master",
            "WITH t AS (SELECT 1) SELECT * FROM t",
            "PRAGMA journal_mode",
            "EXPLAIN SELECT 1",
        ];
        for sql in readonly_queries {
            let result = bridge.execute_readonly_sql(sql).await;
            assert!(result.is_ok(), "{sql:?} must be accepted, got {result:?}");
        }
    }

    // === S20: Concurrent SQLite query (Mutex contention) ===

    #[tokio::test]
    async fn spike_sqlite_concurrent_queries_no_race() {
        let bridge = Arc::new(SpikeSqliteAsyncIoBridge::default());
        let handles: Vec<_> = (0..4)
            .map(|i| {
                let bridge = Arc::clone(&bridge);
                tokio::spawn(async move {
                    let sql = format!("SELECT {i} AS worker");
                    bridge.execute_readonly_sql(&sql).await.expect("query")
                })
            })
            .collect();

        let mut results = Vec::new();
        for h in handles {
            let r = h.await.expect("task join");
            results.push(r);
        }
        // All queries should succeed
        assert_eq!(results.len(), 4);
    }
}
