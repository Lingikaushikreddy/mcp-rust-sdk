//! Streamable HTTP transport for MCP.
//!
//! This transport exposes the MCP server over HTTP using the Streamable HTTP
//! specification. It uses Axum as the HTTP framework and supports POST
//! for receiving requests, GET for SSE notification streams, and DELETE
//! for session termination.
//!
//! The HTTP POST handler uses oneshot channels to correlate JSON-RPC
//! requests with their responses. When a POST arrives with a request
//! (containing an `id`), it sends the message to the server and waits
//! for the response on a oneshot channel keyed by the request ID.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::Json;
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tracing::{debug, info, warn};

use crate::protocol::types::JsonRpcMessage;
use crate::transport::McpTransport;
use crate::types::error::TransportError;

/// Configuration for the HTTP transport.
#[derive(Debug, Clone)]
pub struct HttpConfig {
    /// The address to bind the HTTP server to.
    pub bind_addr: SocketAddr,
    /// The endpoint path for MCP messages (default: "/mcp").
    pub endpoint: String,
    /// Whether to enable session management (default: true).
    pub enable_sessions: bool,
    /// Session timeout duration (default: 30 minutes).
    pub session_timeout: Duration,
    /// Maximum number of concurrent sessions (default: 1000).
    pub max_sessions: usize,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            bind_addr: SocketAddr::from(([0, 0, 0, 0], 3000)),
            endpoint: "/mcp".to_string(),
            enable_sessions: true,
            session_timeout: Duration::from_secs(1800),
            max_sessions: 1000,
        }
    }
}

/// Builder for configuring the HTTP transport.
#[derive(Debug)]
pub struct HttpTransportBuilder {
    config: HttpConfig,
}

impl HttpTransportBuilder {
    /// Creates a new builder with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: HttpConfig::default(),
        }
    }

    /// Sets the bind address.
    #[must_use]
    pub fn bind(mut self, addr: impl Into<SocketAddr>) -> Self {
        self.config.bind_addr = addr.into();
        self
    }

    /// Sets the bind address from a string like "0.0.0.0:3000".
    ///
    /// # Errors
    ///
    /// Returns an error if the address string cannot be parsed.
    pub fn bind_str(mut self, addr: &str) -> Result<Self, TransportError> {
        self.config.bind_addr = addr
            .parse()
            .map_err(|e| TransportError::Other(format!("Invalid bind address: {e}")))?;
        Ok(self)
    }

    /// Sets the MCP endpoint path.
    #[must_use]
    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.config.endpoint = endpoint.into();
        self
    }

    /// Sets whether sessions are enabled.
    #[must_use]
    pub fn enable_sessions(mut self, enable: bool) -> Self {
        self.config.enable_sessions = enable;
        self
    }

    /// Sets the session timeout.
    #[must_use]
    pub fn session_timeout(mut self, timeout: Duration) -> Self {
        self.config.session_timeout = timeout;
        self
    }

    /// Sets the maximum number of concurrent sessions.
    #[must_use]
    pub fn max_sessions(mut self, max: usize) -> Self {
        self.config.max_sessions = max;
        self
    }

    /// Builds the HTTP transport.
    #[must_use]
    pub fn build(self) -> HttpTransport {
        HttpTransport::with_config(self.config)
    }
}

impl Default for HttpTransportBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// State of a single client session.
#[derive(Debug)]
pub struct SessionState {
    /// The session ID.
    pub id: String,
    /// When the session was created.
    pub created_at: Instant,
    /// When the session was last active.
    pub last_active: Instant,
}

/// Shared state for the HTTP transport.
#[derive(Debug)]
struct HttpState {
    /// Active sessions.
    sessions: RwLock<HashMap<String, SessionState>>,
    /// Channel for routing incoming messages to the server.
    incoming_tx: mpsc::Sender<JsonRpcMessage>,
    /// Pending response channels keyed by request ID string.
    pending_responses: Mutex<HashMap<String, oneshot::Sender<JsonRpcMessage>>>,
    /// Config.
    config: HttpConfig,
}

/// MCP transport over Streamable HTTP using Axum.
///
/// Supports POST for requests, GET for SSE streams, and DELETE for
/// session termination.
///
/// The POST handler uses oneshot channels to correlate requests with
/// responses. When a request arrives, a oneshot channel is created and
/// stored by request ID. The server processes the request and sends the
/// response back through the oneshot channel. The HTTP handler awaits
/// the response and returns it to the HTTP client.
pub struct HttpTransport {
    config: HttpConfig,
    state: Arc<HttpState>,
    incoming_rx: Mutex<mpsc::Receiver<JsonRpcMessage>>,
}

impl HttpTransport {
    /// Creates a new HTTP transport with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(HttpConfig::default())
    }

    /// Creates a new HTTP transport with the given configuration.
    #[must_use]
    pub fn with_config(config: HttpConfig) -> Self {
        let (incoming_tx, incoming_rx) = mpsc::channel(256);
        let state = Arc::new(HttpState {
            sessions: RwLock::new(HashMap::new()),
            incoming_tx,
            pending_responses: Mutex::new(HashMap::new()),
            config: config.clone(),
        });
        Self {
            config,
            state,
            incoming_rx: Mutex::new(incoming_rx),
        }
    }

    /// Returns a builder for configuring the HTTP transport.
    #[must_use]
    pub fn builder() -> HttpTransportBuilder {
        HttpTransportBuilder::new()
    }

    /// Builds the Axum router for this transport.
    pub fn router(&self) -> axum::Router {
        let state = Arc::clone(&self.state);
        let endpoint = self.config.endpoint.clone();

        axum::Router::new()
            .route(&endpoint, post(handle_post))
            .route(&endpoint, get(handle_get))
            .route(&endpoint, delete(handle_delete))
            .with_state(state)
    }

    /// Returns the configured bind address.
    #[must_use]
    pub fn bind_addr(&self) -> SocketAddr {
        self.config.bind_addr
    }
}

impl Default for HttpTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for HttpTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpTransport")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl McpTransport for HttpTransport {
    async fn recv(&mut self) -> Result<Option<JsonRpcMessage>, TransportError> {
        let mut rx = self.incoming_rx.lock().await;
        match rx.recv().await {
            Some(msg) => Ok(Some(msg)),
            None => Ok(None),
        }
    }

    async fn send(&self, message: &JsonRpcMessage) -> Result<(), TransportError> {
        // Route the response back to the pending HTTP request via oneshot channel
        if let Some(id) = message.id() {
            let id_str = id.to_string();
            let mut pending = self.state.pending_responses.lock().await;
            if let Some(sender) = pending.remove(&id_str) {
                let _ = sender.send(message.clone());
                return Ok(());
            }
        }
        // If no pending request found (e.g., notification), that's ok
        Ok(())
    }

    async fn send_to(
        &self,
        _session_id: &str,
        message: &JsonRpcMessage,
    ) -> Result<(), TransportError> {
        self.send(message).await
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        debug!("Closing HTTP transport");
        Ok(())
    }
}

/// Extracts the request ID string from a JSON value (for pending response lookup).
fn extract_request_id(body: &serde_json::Value) -> Option<String> {
    body.get("id").map(|id| match id {
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    })
}

/// Handles POST requests to the MCP endpoint.
///
/// For requests (messages with an `id`), this handler creates a oneshot
/// channel, routes the message to the server, and waits for the response
/// on the oneshot channel. The actual JSON-RPC result is returned to the
/// HTTP client.
///
/// For notifications (messages without an `id`), this handler routes the
/// message and returns `202 Accepted` immediately.
#[allow(clippy::too_many_lines)]
async fn handle_post(
    State(state): State<Arc<HttpState>>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    debug!("Received POST request");

    // Parse the JSON-RPC message
    let message: JsonRpcMessage = match serde_json::from_value(body.clone()) {
        Ok(msg) => msg,
        Err(e) => {
            warn!(error = %e, "Failed to parse JSON-RPC message from POST body");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {e}")
                    }
                })),
            );
        }
    };

    // Create or retrieve session
    let mut session_id_header = None;
    if state.config.enable_sessions {
        let existing_session_id = headers
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        if let Some(ref sid) = existing_session_id {
            let sessions = state.sessions.read().await;
            if !sessions.contains_key(sid) {
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": {
                            "code": -32600,
                            "message": "Unknown session"
                        }
                    })),
                );
            }
            session_id_header = Some(sid.clone());
        } else if message.method() == Some("initialize") {
            // Create a new session for initialize requests
            let new_sid = uuid::Uuid::new_v4().to_string();
            let mut sessions = state.sessions.write().await;
            sessions.insert(
                new_sid.clone(),
                SessionState {
                    id: new_sid.clone(),
                    created_at: Instant::now(),
                    last_active: Instant::now(),
                },
            );
            session_id_header = Some(new_sid);
            info!("Created new session");
        }
    }

    // Determine if this is a request (has id) or notification (no id)
    let request_id_str = extract_request_id(&body);

    if let Some(ref id_str) = request_id_str {
        // This is a request -- set up a oneshot channel for the response
        let (response_tx, response_rx) = oneshot::channel();

        {
            let mut pending = state.pending_responses.lock().await;
            pending.insert(id_str.clone(), response_tx);
        }

        // Route the message to the server
        if let Err(e) = state.incoming_tx.send(message).await {
            warn!(error = %e, "Failed to route incoming message");
            // Clean up the pending response
            let mut pending = state.pending_responses.lock().await;
            pending.remove(id_str);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32603,
                        "message": "Internal error"
                    }
                })),
            );
        }

        // Wait for the response from the server, with a 30-second timeout
        // to prevent the HTTP handler from hanging indefinitely.
        match tokio::time::timeout(Duration::from_secs(30), response_rx).await {
            Ok(Ok(response_msg)) => {
                let response_json = serde_json::to_value(&response_msg).unwrap_or_else(|_| {
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": {
                            "code": -32603,
                            "message": "Failed to serialize response"
                        }
                    })
                });

                // Note: In a full implementation, we would set Mcp-Session-Id header.
                // For now, return the response with 200 OK.
                let _ = session_id_header; // used in future header setting
                (StatusCode::OK, Json(response_json))
            }
            Ok(Err(_)) => {
                // The server dropped the sender without sending a response
                warn!("Server did not send a response for request");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": {
                            "code": -32603,
                            "message": "Internal error: no response from server"
                        }
                    })),
                )
            }
            Err(_) => {
                // Timeout waiting for the server to respond
                warn!("Timeout waiting for server response (30s)");
                // Clean up the pending response entry
                let mut pending = state.pending_responses.lock().await;
                pending.remove(id_str);
                (
                    StatusCode::GATEWAY_TIMEOUT,
                    Json(serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": body.get("id").cloned().unwrap_or(serde_json::Value::Null),
                        "error": {
                            "code": -32603,
                            "message": "Request timed out"
                        }
                    })),
                )
            }
        }
    } else {
        // This is a notification -- route it and return 202 Accepted
        if let Err(e) = state.incoming_tx.send(message).await {
            warn!(error = %e, "Failed to route notification");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32603,
                        "message": "Internal error"
                    }
                })),
            );
        }

        (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": null,
                "result": {}
            })),
        )
    }
}

/// Handles GET requests for SSE notification streams.
async fn handle_get(State(_state): State<Arc<HttpState>>) -> impl IntoResponse {
    info!("SSE stream requested via GET");
    // In a full implementation, this would return an SSE stream.
    // For Phase 1, we return 405 Method Not Allowed since SSE is P2.
    StatusCode::METHOD_NOT_ALLOWED
}

/// Handles DELETE requests for session termination.
async fn handle_delete(
    State(state): State<Arc<HttpState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let session_id = headers.get("mcp-session-id").and_then(|v| v.to_str().ok());

    if let Some(sid) = session_id {
        let mut sessions = state.sessions.write().await;
        if sessions.remove(sid).is_some() {
            info!(session_id = %sid, "Session terminated");
            StatusCode::NO_CONTENT
        } else {
            warn!(session_id = %sid, "Attempted to delete unknown session");
            StatusCode::NOT_FOUND
        }
    } else {
        warn!("DELETE request missing Mcp-Session-Id header");
        StatusCode::BAD_REQUEST
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    #[test]
    fn test_default_config() {
        let config = HttpConfig::default();
        assert_eq!(config.endpoint, "/mcp");
        assert!(config.enable_sessions);
        assert_eq!(config.max_sessions, 1000);
    }

    #[test]
    fn test_builder() {
        let transport = HttpTransport::builder()
            .bind(SocketAddr::from(([127, 0, 0, 1], 8080)))
            .endpoint("/api/mcp")
            .max_sessions(500)
            .build();

        assert_eq!(transport.config.bind_addr.port(), 8080);
        assert_eq!(transport.config.endpoint, "/api/mcp");
        assert_eq!(transport.config.max_sessions, 500);
    }

    #[test]
    fn test_router_creation() {
        let transport = HttpTransport::new();
        let _router = transport.router();
        // Router creation should not panic
    }

    #[tokio::test]
    async fn test_post_invalid_json() {
        let transport = HttpTransport::new();
        let router = transport.router();

        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .body(Body::from(r"not valid json"))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_post_notification_returns_accepted() {
        let transport = HttpTransport::new();
        let router = transport.router();

        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });

        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&notification).unwrap()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn test_delete_without_session_header() {
        let transport = HttpTransport::new();
        let router = transport.router();

        let request = Request::builder()
            .method("DELETE")
            .uri("/mcp")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_delete_unknown_session() {
        let transport = HttpTransport::new();
        let router = transport.router();

        let request = Request::builder()
            .method("DELETE")
            .uri("/mcp")
            .header("mcp-session-id", "nonexistent-session")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_returns_method_not_allowed() {
        let transport = HttpTransport::new();
        let router = transport.router();

        let request = Request::builder()
            .method("GET")
            .uri("/mcp")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn test_post_request_with_response() {
        // Test that a request gets routed and the response is returned
        let transport = HttpTransport::new();
        let transport_state = Arc::clone(&transport.state);
        let router = transport.router();

        // Spawn a task that reads from the transport and sends a response
        let state_clone = Arc::clone(&transport_state);
        tokio::spawn(async move {
            // Wait for the incoming message
            let mut incoming_rx = transport.incoming_rx.lock().await;
            if let Some(msg) = incoming_rx.recv().await {
                // Send back a response through the pending response channel
                if let Some(id) = msg.id() {
                    let id_str = id.to_string();
                    let mut pending = state_clone.pending_responses.lock().await;
                    if let Some(sender) = pending.remove(&id_str) {
                        let response = JsonRpcMessage::response(
                            id.clone(),
                            serde_json::json!({"status": "ok"}),
                        );
                        let _ = sender.send(response);
                    }
                }
            }
        });

        let init_request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "ping"
        });

        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&init_request).unwrap()))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["result"]["status"], "ok");
        assert_eq!(json["id"], 1);
    }
}
