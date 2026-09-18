//! Request context for MCP handlers.
//!
//! The context provides metadata about the current request, including
//! the request ID, session information, and a mechanism for sending
//! progress notifications back to the client.

use serde_json::Value;
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;

/// Context provided to tool handlers during execution.
///
/// The context carries request metadata and allows the handler to
/// report progress or access session-level information.
///
/// # Cancellation
///
/// Tool handlers can check `is_cancelled()` to detect when a client
/// sends a `notifications/cancelled` notification. Long-running tools
/// should poll this periodically and return early if cancelled.
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// The JSON-RPC request ID associated with this call.
    pub request_id: Option<String>,
    /// The session ID for HTTP transport, if applicable.
    pub session_id: Option<String>,
    /// Arbitrary metadata attached to the request.
    pub metadata: HashMap<String, Value>,
    /// Cancellation token for cooperative cancellation.
    cancellation_token: CancellationToken,
}

impl Default for ToolContext {
    fn default() -> Self {
        Self {
            request_id: None,
            session_id: None,
            metadata: HashMap::new(),
            cancellation_token: CancellationToken::new(),
        }
    }
}

impl ToolContext {
    /// Creates a new context with the given request ID.
    #[must_use]
    pub fn new(request_id: impl Into<String>) -> Self {
        Self {
            request_id: Some(request_id.into()),
            session_id: None,
            metadata: HashMap::new(),
            cancellation_token: CancellationToken::new(),
        }
    }

    /// Creates a new context with the given request ID and cancellation token.
    #[must_use]
    pub fn with_cancellation(request_id: impl Into<String>, token: CancellationToken) -> Self {
        Self {
            request_id: Some(request_id.into()),
            session_id: None,
            metadata: HashMap::new(),
            cancellation_token: token,
        }
    }

    /// Sets the session ID.
    #[must_use]
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Inserts a metadata value.
    pub fn insert_metadata(&mut self, key: impl Into<String>, value: Value) {
        self.metadata.insert(key.into(), value);
    }

    /// Retrieves a metadata value by key.
    #[must_use]
    pub fn get_metadata(&self, key: &str) -> Option<&Value> {
        self.metadata.get(key)
    }

    /// Returns whether this request has been cancelled.
    ///
    /// Tool handlers should check this periodically during long-running
    /// operations and return early with an error if true.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancellation_token.is_cancelled()
    }

    /// Returns a reference to the cancellation token.
    ///
    /// This can be used with `tokio::select!` to race a long-running
    /// operation against cancellation.
    #[must_use]
    pub fn cancellation_token(&self) -> &CancellationToken {
        &self.cancellation_token
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_default() {
        let ctx = ToolContext::default();
        assert!(ctx.request_id.is_none());
        assert!(ctx.session_id.is_none());
        assert!(ctx.metadata.is_empty());
        assert!(!ctx.is_cancelled());
    }

    #[test]
    fn test_context_builder() {
        let ctx = ToolContext::new("req-1").with_session_id("sess-abc");
        assert_eq!(ctx.request_id.as_deref(), Some("req-1"));
        assert_eq!(ctx.session_id.as_deref(), Some("sess-abc"));
    }

    #[test]
    fn test_context_metadata() {
        let mut ctx = ToolContext::default();
        ctx.insert_metadata("key", serde_json::json!("value"));
        assert_eq!(ctx.get_metadata("key"), Some(&serde_json::json!("value")));
        assert!(ctx.get_metadata("missing").is_none());
    }

    #[test]
    fn test_context_cancellation() {
        let token = CancellationToken::new();
        let ctx = ToolContext::with_cancellation("req-1", token.clone());
        assert!(!ctx.is_cancelled());
        token.cancel();
        assert!(ctx.is_cancelled());
    }
}
