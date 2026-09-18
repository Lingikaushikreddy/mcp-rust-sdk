//! Server-Sent Events (SSE) support for the HTTP transport.
//!
//! This module provides SSE streaming functionality used by the Streamable
//! HTTP transport to push server-initiated messages (notifications, progress
//! updates, streaming tool results) to connected clients.

use std::fmt::Write;

use serde::Serialize;
use tokio::sync::mpsc;
use tracing::debug;

/// A single SSE event.
#[derive(Debug, Clone)]
pub struct SseEvent {
    /// The event type (default: "message").
    pub event: String,
    /// The event data (typically a JSON string).
    pub data: String,
    /// Optional event ID for Last-Event-Id resumability.
    pub id: Option<String>,
}

impl SseEvent {
    /// Creates a new SSE event with the "message" event type.
    #[must_use]
    pub fn message(data: impl Into<String>) -> Self {
        Self {
            event: "message".to_string(),
            data: data.into(),
            id: None,
        }
    }

    /// Creates a new SSE event with a custom event type.
    #[must_use]
    pub fn with_event(event: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            event: event.into(),
            data: data.into(),
            id: None,
        }
    }

    /// Sets the event ID.
    #[must_use]
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Creates an SSE event from a serializable value.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn from_json<T: Serialize>(value: &T) -> Result<Self, serde_json::Error> {
        let data = serde_json::to_string(value)?;
        Ok(Self::message(data))
    }

    /// Formats the event as an SSE text block.
    #[must_use]
    pub fn to_sse_string(&self) -> String {
        let mut result = String::new();

        if let Some(ref id) = self.id {
            let _ = writeln!(result, "id: {id}");
        }

        let _ = writeln!(result, "event: {}", self.event);
        let _ = writeln!(result, "data: {}", self.data);
        result.push('\n');

        result
    }
}

/// Manages SSE connections for a session.
///
/// Each connected SSE client gets a sender half of an mpsc channel.
/// The server pushes events through the sender, and the HTTP handler
/// streams them to the client.
#[derive(Debug)]
pub struct SseManager {
    /// Senders for active SSE connections.
    senders: Vec<mpsc::Sender<SseEvent>>,
}

impl SseManager {
    /// Creates a new SSE manager with no active connections.
    #[must_use]
    pub fn new() -> Self {
        Self {
            senders: Vec::new(),
        }
    }

    /// Registers a new SSE client and returns the receiver end.
    ///
    /// The caller should stream events from the returned receiver
    /// to the HTTP response.
    pub fn subscribe(&mut self, buffer_size: usize) -> mpsc::Receiver<SseEvent> {
        let (tx, rx) = mpsc::channel(buffer_size);
        self.senders.push(tx);
        debug!(subscriber_count = self.senders.len(), "New SSE subscriber");
        rx
    }

    /// Sends an event to all connected SSE clients.
    ///
    /// Disconnected clients (closed channels) are automatically removed.
    pub async fn broadcast(&mut self, event: SseEvent) {
        let mut i = 0;
        while i < self.senders.len() {
            if self.senders[i].send(event.clone()).await.is_err() {
                debug!("Removing disconnected SSE subscriber");
                self.senders.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }

    /// Returns the number of active SSE connections.
    #[must_use]
    pub fn subscriber_count(&self) -> usize {
        self.senders.len()
    }
}

impl Default for SseManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_event_message() {
        let event = SseEvent::message(r#"{"hello":"world"}"#);
        assert_eq!(event.event, "message");
        assert_eq!(event.data, r#"{"hello":"world"}"#);
        assert!(event.id.is_none());
    }

    #[test]
    fn test_sse_event_format() {
        let event = SseEvent::message("test data").with_id("42");
        let formatted = event.to_sse_string();
        assert!(formatted.contains("id: 42\n"));
        assert!(formatted.contains("event: message\n"));
        assert!(formatted.contains("data: test data\n"));
        assert!(formatted.ends_with("\n\n"));
    }

    #[test]
    fn test_sse_event_from_json() {
        let value = serde_json::json!({"key": "value"});
        let event = SseEvent::from_json(&value).expect("serialize");
        assert_eq!(event.event, "message");
        assert!(event.data.contains("key"));
    }

    #[tokio::test]
    async fn test_sse_manager_subscribe() {
        let mut manager = SseManager::new();
        let _rx = manager.subscribe(16);
        assert_eq!(manager.subscriber_count(), 1);
    }

    #[tokio::test]
    async fn test_sse_manager_broadcast() {
        let mut manager = SseManager::new();
        let mut rx = manager.subscribe(16);

        manager.broadcast(SseEvent::message("hello")).await;

        let event = rx.recv().await.expect("receive");
        assert_eq!(event.data, "hello");
    }

    #[tokio::test]
    async fn test_sse_manager_removes_disconnected() {
        let mut manager = SseManager::new();
        let rx = manager.subscribe(1);
        drop(rx); // Disconnect

        manager.broadcast(SseEvent::message("hello")).await;
        assert_eq!(manager.subscriber_count(), 0);
    }
}
