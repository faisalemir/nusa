//! Async task offloading system with actual execution.
//!
//! Skills applied:
//! - `m07-concurrency`: Tokio task spawn, channel-based result routing
//! - `m13-domain-error`: Task timeout, error propagation

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::sync::oneshot;

/// Task types that can be offloaded from PHP to Rust.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum OffloadTask {
    /// HTTP GET/POST request
    HttpRequest {
        method: String,
        url: String,
        headers: HashMap<String, String>,
        body: Option<Vec<u8>>,
    },
    /// File I/O operation
    FileOperation {
        operation: String,
        path: String,
        data: Option<Vec<u8>>,
    },
    /// Custom task with JSON payload
    Custom {
        task_type: String,
        payload: serde_json::Value,
    },
}

/// Result of an offloaded task.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaskResult {
    pub success: bool,
    pub data: Vec<u8>,
    pub error: Option<String>,
}

/// Task status for queries.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskStatus {
    pub task_id: String,
    pub completed: bool,
    pub result: Option<TaskResult>,
}

/// Stored task info for status queries.
struct StoredTask {
    result: Option<TaskResult>,
}

/// Task manager for async offloading (D1: actual execution).
pub struct TaskManager {
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<TaskResult>>>>,
    completed: Arc<Mutex<HashMap<String, StoredTask>>>,
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskManager {
    pub fn new() -> Self {
        Self {
            pending: Arc::new(Mutex::new(HashMap::new())),
            completed: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Submit a task for async execution — actually spawns the tokio task.
    /// Works both inside and outside a tokio runtime (m07-concurrency).
    pub fn submit(&self, task: OffloadTask) -> (String, oneshot::Receiver<TaskResult>) {
        let (tx, rx) = oneshot::channel();
        let task_id = uuid::Uuid::new_v4().to_string();
        let completed_store = self.completed.clone();
        let pending = self.pending.clone();

        pending.lock().insert(task_id.clone(), tx);

        let task_id_clone = task_id.clone();

        // Actually execute the task asynchronously (m07-concurrency)
        // If a tokio runtime is available, use it; otherwise spawn a thread.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let result = execute_task(task).await;
                complete_task(&completed_store, &pending, &task_id_clone, result);
            });
        } else {
            // No runtime — spawn a thread for sync execution
            // For async tasks, create a single-threaded runtime
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to build tokio runtime");
                let result = rt.block_on(execute_task(task));
                complete_task(&completed_store, &pending, &task_id_clone, result);
            });
        }

        (task_id, rx)
    }

    /// Query task status.
    pub fn status(&self, task_id: &str) -> TaskStatus {
        let completed = self.completed.lock();
        if let Some(stored) = completed.get(task_id) {
            TaskStatus {
                task_id: task_id.to_string(),
                completed: true,
                result: stored.result.clone(),
            }
        } else {
            TaskStatus {
                task_id: task_id.to_string(),
                completed: false,
                result: None,
            }
        }
    }
}

/// Execute an offloaded task with a timeout.
async fn execute_task(task: OffloadTask) -> TaskResult {
    match task {
        OffloadTask::HttpRequest {
            method,
            url,
            headers,
            body,
        } => tokio::time::timeout(
            Duration::from_secs(60),
            execute_http_task(method, url, headers, body),
        )
        .await
        .unwrap_or_else(|_| TaskResult {
            success: false,
            data: vec![],
            error: Some("HTTP task timed out (60s)".into()),
        }),
        OffloadTask::FileOperation {
            operation,
            path,
            data,
        } => tokio::time::timeout(
            Duration::from_secs(30),
            execute_file_task(operation, path, data),
        )
        .await
        .unwrap_or_else(|_| TaskResult {
            success: false,
            data: vec![],
            error: Some("File operation timed out (30s)".into()),
        }),
        OffloadTask::Custom { task_type, payload } => TaskResult {
            success: false,
            data: vec![],
            error: Some(format!(
                "Custom task type '{task_type}' not yet implemented. Payload: {payload}"
            )),
        },
    }
}

async fn execute_http_task(
    method: String,
    url: String,
    headers: HashMap<String, String>,
    body: Option<Vec<u8>>,
) -> TaskResult {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(55))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))
        .unwrap_or_else(|_| reqwest::Client::new());

    let mut req = match method.to_uppercase().as_str() {
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "DELETE" => client.delete(&url),
        "PATCH" => client.patch(&url),
        _ => client.get(&url),
    };

    for (k, v) in &headers {
        req = req.header(k, v);
    }

    if let Some(body_bytes) = body {
        req = req.body(body_bytes);
    }

    match req.send().await {
        Ok(resp) => {
            let status = resp.status();
            let bytes = resp.bytes().await;
            match bytes {
                Ok(b) => TaskResult {
                    success: status.is_success(),
                    data: b.to_vec(),
                    error: if status.is_success() {
                        None
                    } else {
                        Some(format!("HTTP {}", status))
                    },
                },
                Err(e) => TaskResult {
                    success: false,
                    data: vec![],
                    error: Some(format!("Failed to read response body: {e}")),
                },
            }
        }
        Err(e) => TaskResult {
            success: false,
            data: vec![],
            error: Some(format!("HTTP request failed: {e}")),
        },
    }
}

async fn execute_file_task(operation: String, path: String, data: Option<Vec<u8>>) -> TaskResult {
    match operation.to_lowercase().as_str() {
        "read" => match tokio::fs::read(&path).await {
            Ok(bytes) => TaskResult {
                success: true,
                data: bytes,
                error: None,
            },
            Err(e) => TaskResult {
                success: false,
                data: vec![],
                error: Some(format!("Failed to read file: {e}")),
            },
        },
        "write" => {
            if let Some(data_bytes) = data {
                let path_buf = PathBuf::from(&path);
                if let Some(parent) = path_buf.parent()
                    && let Err(e) = tokio::fs::create_dir_all(parent).await
                {
                    return TaskResult {
                        success: false,
                        data: vec![],
                        error: Some(format!("Failed to create directory: {e}")),
                    };
                }
                match tokio::fs::write(&path, &data_bytes).await {
                    Ok(()) => TaskResult {
                        success: true,
                        data: vec![],
                        error: None,
                    },
                    Err(e) => TaskResult {
                        success: false,
                        data: vec![],
                        error: Some(format!("Failed to write file: {e}")),
                    },
                }
            } else {
                TaskResult {
                    success: false,
                    data: vec![],
                    error: Some("No data provided for write operation".into()),
                }
            }
        }
        "delete" => match tokio::fs::remove_file(&path).await {
            Ok(()) => TaskResult {
                success: true,
                data: vec![],
                error: None,
            },
            Err(e) => TaskResult {
                success: false,
                data: vec![],
                error: Some(format!("Failed to delete file: {e}")),
            },
        },
        _ => TaskResult {
            success: false,
            data: vec![],
            error: Some(format!("Unknown file operation: {operation}")),
        },
    }
}

/// Store task result and send to awaiting receiver.
fn complete_task(
    completed: &Arc<Mutex<HashMap<String, StoredTask>>>,
    pending: &Arc<Mutex<HashMap<String, oneshot::Sender<TaskResult>>>>,
    task_id: &str,
    result: TaskResult,
) {
    completed.lock().insert(
        task_id.to_string(),
        StoredTask {
            result: Some(result.clone()),
        },
    );

    if let Some(tx) = pending.lock().remove(task_id) {
        let _ = tx.send(result);
    }
}
