//! W3C Trace Context for distributed tracing propagation (Gateway → IPC → Laravel).
//!
//! Skills: `m07-concurrency`, `domain-cloud-native`

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// W3C Trace Context for distributed tracing propagation (Gateway → IPC → Laravel).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TraceContext {
    pub traceparent: String,
    pub tracestate: Option<String>,
}

impl TraceContext {
    /// Extract trace context from a raw header map.
    pub fn from_raw_headers(headers: &HashMap<String, Vec<String>>) -> Self {
        let traceparent = headers
            .get("traceparent")
            .and_then(|v| v.first().cloned())
            .unwrap_or_default();

        let tracestate = headers
            .get("tracestate")
            .and_then(|v| v.first().cloned());

        Self {
            traceparent,
            tracestate,
        }
    }

    /// Extract from http::HeaderMap (gateway middleware).
    pub fn from_http_headers(headers: &http::HeaderMap) -> Self {
        let traceparent = headers
            .get("traceparent")
            .and_then(|v| v.to_str().ok())
            .map(String::from)
            .unwrap_or_default();

        let tracestate = headers
            .get("tracestate")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        Self {
            traceparent,
            tracestate,
        }
    }

    /// Inject trace context into the headers map.
    pub fn inject_into(self, headers: &mut HashMap<String, Vec<String>>) {
        if !self.traceparent.is_empty() {
            headers.insert("traceparent".into(), vec![self.traceparent]);
        }
        if let Some(ts) = self.tracestate {
            headers.insert("tracestate".into(), vec![ts]);
        }
    }
}
