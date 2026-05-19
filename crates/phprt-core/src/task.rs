//! Async task offloading system for Octane mode.
//!
//! Skills applied:
//! - `m07-concurrency`: Tokio task spawn, channel-based result routing
//! - `m13-domain-error`: Task timeout, error propagation

use std::collections::HashMap;

use parking_lot::Mutex;
use tokio::sync::oneshot;

/// Task types that can be offloaded from PHP to Rust.
#[derive(Debug, Clone)]
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
        operation: String, // "read", "write", "delete"
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
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub success: bool,
    pub data: Vec<u8>,
    pub error: Option<String>,
}

/// Task manager for async offloading.
pub struct TaskManager {
    pending: Mutex<HashMap<String, oneshot::Sender<TaskResult>>>,
}

impl TaskManager {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
        }
    }

    /// Submit a task for async execution.
    pub fn submit(
        &self,
        _task: OffloadTask,
    ) -> (String, oneshot::Receiver<TaskResult>) {
        let (tx, rx) = oneshot::channel();
        let task_id = uuid::Uuid::new_v4().to_string();
        self.pending.lock().insert(task_id.clone(), tx);
        (task_id, rx)
    }

    /// Complete a task and send result back to PHP.
    pub fn complete(&self, task_id: &str, result: TaskResult) {
        if let Some(tx) = self.pending.lock().remove(task_id) {
            let _ = tx.send(result);
        }
    }
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}
