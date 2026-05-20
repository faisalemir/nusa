//! Server-Sent Events handler for one-way real-time notifications.
//! Blueprint 6 F2: SSE endpoint for dashboard notifications, progress bars, live data feeds.
//!
//! Skills applied:
//! - `domain-web`: SSE protocol, EventSource interface, proper text/event-stream headers
//! - `m07-concurrency`: tokio broadcast channel for fan-out to multiple subscribers
//! - `m13-domain-error`: Graceful handling of lagged/closed channels

use std::convert::Infallible;

use axum::response::sse::Event;
use futures::stream::Stream;
use tokio::sync::broadcast;
use tracing::info;

/// SSE broadcast channel manager.
///
/// domain-web: Provides EventSource-compatible streaming responses.
/// m07-concurrency: Broadcast channel fans out events to all subscribers.
pub struct SseManager {
    sender: broadcast::Sender<String>,
}

impl SseManager {
    pub fn new() -> Self {
        // m07-concurrency: 256-event buffer prevents message loss for slow consumers
        let (sender, _) = broadcast::channel(256);
        Self { sender }
    }

    /// Get an SSE event stream (domain-web).
    pub fn stream(&self) -> impl Stream<Item = Result<Event, Infallible>> + use<> {
        let mut rx = self.sender.subscribe();

        // m13-domain-error: Handle lagged/closed channels gracefully
        async_stream::stream! {
            loop {
                match rx.recv().await {
                    Ok(data) => {
                        yield Ok(Event::default().data(data));
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        info!("SSE client lagged by {} messages", n);
                        yield Ok(Event::default().data("{\"type\":\"resync\"}"));
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    /// Send an event to all SSE subscribers (domain-web).
    pub fn send(&self, event: &str) {
        let _ = self.sender.send(event.to_string());
    }
}

impl Default for SseManager {
    fn default() -> Self {
        Self::new()
    }
}
