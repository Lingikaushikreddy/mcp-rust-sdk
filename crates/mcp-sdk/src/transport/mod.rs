//! Transport layer abstractions for MCP communication.
//!
//! This module defines the [`McpTransport`] trait and its implementations
//! for stdio and HTTP transports. The transport layer is responsible for
//! reading and writing JSON-RPC messages between the MCP server and client.

pub mod stdio;

#[cfg(feature = "http")]
pub mod http;

#[cfg(feature = "sse")]
pub mod sse;

use async_trait::async_trait;

use crate::protocol::types::JsonRpcMessage;
use crate::types::error::TransportError;

/// Trait abstracting the transport layer for MCP communication.
///
/// Implementations handle the serialization and I/O of JSON-RPC messages
/// over a specific transport mechanism (stdio, HTTP, etc.).
#[allow(
    clippy::double_must_use,
    reason = "async_trait adds must_use to its generated Future-returning methods"
)]
#[async_trait]
pub trait McpTransport: Send + Sync + 'static {
    /// Receives the next JSON-RPC message from the client.
    ///
    /// Returns `Ok(None)` when the transport is cleanly closed (e.g., EOF on stdin).
    async fn recv(&mut self) -> Result<Option<JsonRpcMessage>, TransportError>;

    /// Sends a JSON-RPC message to the client.
    async fn send(&self, message: &JsonRpcMessage) -> Result<(), TransportError>;

    /// Sends a JSON-RPC message to a specific session.
    ///
    /// The default implementation ignores the session ID and delegates to [`send`](Self::send).
    /// Multi-session transports (like HTTP) should override this.
    async fn send_to(
        &self,
        _session_id: &str,
        message: &JsonRpcMessage,
    ) -> Result<(), TransportError> {
        self.send(message).await
    }

    /// Closes the transport gracefully.
    async fn close(&mut self) -> Result<(), TransportError>;
}
