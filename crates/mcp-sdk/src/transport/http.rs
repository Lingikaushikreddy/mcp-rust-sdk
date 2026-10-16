//! Streamable HTTP transport for MCP.
//!
//! This transport exposes the MCP server over HTTP following the Streamable
//! HTTP transport of the MCP 2025-11-25 specification. It uses Axum as the
//! HTTP framework:
//!
//! - `POST` carries one JSON-RPC message. Requests are answered with a single
//!   `application/json` response; notifications and responses get
//!   `202 Accepted` with no body.
//! - `GET` returns `405 Method Not Allowed`: this transport does not offer a
//!   server-to-client SSE stream, which the specification permits.
//! - `DELETE` terminates a session.
//!
//! Every request is checked against an allow-list of host names in its
//! `Host` and `Origin` headers (DNS rebinding protection) and against the
//! protocol versions this SDK supports (`MCP-Protocol-Version`).
//!
//! The server sees each HTTP request under a transport-assigned request id,
//! so two sessions that both use id `1` cannot receive each other's
//! responses; the client's own id is restored on the way out.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tracing::{debug, info, warn};

use crate::protocol::capabilities::SUPPORTED_PROTOCOL_VERSIONS;
use crate::protocol::types::{decode_jsonrpc_message, JsonRpcMessage, RequestId};
use crate::transport::McpTransport;
use crate::types::error::{JsonRpcError, TransportError};

const SESSION_HEADER: &str = "mcp-session-id";
const PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

/// Configuration for the HTTP transport.
#[derive(Debug, Clone)]
pub struct HttpConfig {
    /// The address to bind the HTTP server to (default: `127.0.0.1:3000`).
    pub bind_addr: SocketAddr,
    /// The endpoint path for MCP messages (default: "/mcp").
    pub endpoint: String,
    /// Whether to enable session management (default: true).
    pub enable_sessions: bool,
    /// Session timeout duration (default: 30 minutes).
    pub session_timeout: Duration,
    /// Maximum number of concurrent sessions (default: 1000).
    pub max_sessions: usize,
    /// Host names accepted in the `Host` and `Origin` headers, compared
    /// case-insensitively and without the port (default: `localhost`,
    /// `127.0.0.1` and `[::1]`). `None` disables the check.
    pub allowed_hosts: Option<Vec<String>>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            bind_addr: SocketAddr::from(([127, 0, 0, 1], 3000)),
            endpoint: "/mcp".to_string(),
            enable_sessions: true,
            session_timeout: Duration::from_secs(1800),
            max_sessions: 1000,
            allowed_hosts: Some(
                ["localhost", "127.0.0.1", "[::1]"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
            ),
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

    /// Sets the bind address from a string like "127.0.0.1:3000".
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

    /// Adds host names accepted in the `Host` and `Origin` headers, on top of
    /// the local defaults. Use this for the public name of a server deployed
    /// behind a reverse proxy.
    #[must_use]
    pub fn allowed_hosts<I, S>(mut self, hosts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.config
            .allowed_hosts
            .get_or_insert_with(Vec::new)
            .extend(hosts.into_iter().map(Into::into));
        self
    }

    /// Disables `Host` and `Origin` validation. Only do this when another
    /// layer (such as a reverse proxy) performs the check: the specification
    /// requires servers to validate `Origin` to prevent DNS rebinding.
    #[must_use]
    pub fn allow_any_host(mut self) -> Self {
        self.config.allowed_hosts = None;
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

/// A request forwarded to the server and still waiting for its response.
#[derive(Debug)]
struct PendingRequest {
    /// The id the client used, restored on the response.
    client_id: RequestId,
    session: Option<String>,
    respond: oneshot::Sender<JsonRpcMessage>,
}

/// Requests in flight, keyed by the transport-assigned id the server sees.
#[derive(Debug, Default)]
struct InFlight {
    by_server_id: HashMap<String, PendingRequest>,
    /// `(session, client id)` to server id, for `notifications/cancelled`.
    by_client_id: HashMap<(Option<String>, String), String>,
}

/// Shared state for the HTTP transport.
#[derive(Debug)]
struct HttpState {
    sessions: RwLock<HashMap<String, SessionState>>,
    incoming_tx: mpsc::Sender<JsonRpcMessage>,
    in_flight: Mutex<InFlight>,
    next_id: AtomicU64,
    config: HttpConfig,
}

/// MCP transport over Streamable HTTP using Axum.
///
/// Serve [`HttpTransport::router`] with Axum and pass the transport to
/// [`McpServer::run`](crate::server::McpServer::run):
///
/// ```rust,no_run
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use mcp_sdk::server::McpServer;
/// use mcp_sdk::transport::http::HttpTransport;
///
/// let transport = HttpTransport::new();
/// let listener = tokio::net::TcpListener::bind(transport.bind_addr()).await?;
/// let router = transport.router();
/// tokio::spawn(async move { axum::serve(listener, router).await });
/// McpServer::builder().build().run(transport).await?;
/// # Ok(())
/// # }
/// ```
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
            in_flight: Mutex::new(InFlight::default()),
            next_id: AtomicU64::new(1),
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
            .route(
                &endpoint,
                post(handle_post).get(handle_get).delete(handle_delete),
            )
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
        Ok(rx.recv().await)
    }

    async fn send(&self, message: &JsonRpcMessage) -> Result<(), TransportError> {
        let Some(server_id) = message.id().map(ToString::to_string) else {
            // Without a GET stream there is no channel for server-initiated
            // notifications.
            debug!("Dropping server notification: no SSE stream on this transport");
            return Ok(());
        };
        let pending = {
            let mut in_flight = self.state.in_flight.lock().await;
            let pending = in_flight.by_server_id.remove(&server_id);
            if let Some(p) = &pending {
                in_flight
                    .by_client_id
                    .remove(&(p.session.clone(), p.client_id.to_string()));
            }
            pending
        };
        if let Some(p) = pending {
            let _ = p.respond.send(with_id(message.clone(), p.client_id));
        } else {
            debug!(id = %server_id, "No pending HTTP request for response");
        }
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

/// Returns `message` with its id replaced.
fn with_id(message: JsonRpcMessage, id: RequestId) -> JsonRpcMessage {
    match message {
        JsonRpcMessage::Request(mut r) => {
            r.id = id;
            JsonRpcMessage::Request(r)
        }
        JsonRpcMessage::Response(mut r) => {
            r.id = id;
            JsonRpcMessage::Response(r)
        }
        JsonRpcMessage::ErrorResponse(mut r) => {
            r.id = Some(id);
            JsonRpcMessage::ErrorResponse(r)
        }
        JsonRpcMessage::Notification(n) => JsonRpcMessage::Notification(n),
    }
}

/// An HTTP error whose body is a JSON-RPC error response without an id
/// (2025-11-25 basic/transports.mdx).
fn error_reply(status: StatusCode, error: JsonRpcError) -> Response {
    json_reply(status, &JsonRpcMessage::error_response_without_id(error))
}

fn json_reply(status: StatusCode, message: &JsonRpcMessage) -> Response {
    let body = serde_json::to_vec(message).unwrap_or_default();
    (
        status,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )],
        body,
    )
        .into_response()
}

fn accepted() -> Response {
    StatusCode::ACCEPTED.into_response()
}

/// Extracts the host name, without port, from a `Host` header value or the
/// authority of an `Origin`.
fn host_name(authority: &str) -> &str {
    if authority.starts_with('[') {
        authority
            .find(']')
            .map_or(authority, |end| &authority[..=end])
    } else {
        authority.split(':').next().unwrap_or(authority)
    }
}

/// DNS rebinding protection: `Host` and, when present, `Origin` must name an
/// allowed host. 2025-11-25 basic/transports.mdx: "Servers MUST validate the
/// Origin header on all incoming connections".
fn host_or_origin_rejection(config: &HttpConfig, headers: &HeaderMap) -> Option<Response> {
    let allowed = config.allowed_hosts.as_ref()?;
    let is_allowed = |name: &str| allowed.iter().any(|a| a.eq_ignore_ascii_case(name));
    let forbidden = |what: &str, value: &str| {
        warn!(%what, %value, "Rejected request from a host that is not allowed");
        error_reply(
            StatusCode::FORBIDDEN,
            JsonRpcError::invalid_request(Some(format!("{what} not allowed: {value}"))),
        )
    };

    if let Some(host) = headers.get(header::HOST) {
        let host = host.to_str().unwrap_or_default();
        if !is_allowed(host_name(host)) {
            return Some(forbidden("Host", host));
        }
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        let origin = origin.to_str().unwrap_or_default();
        let authority = origin
            .split_once("://")
            .map(|(_, rest)| rest.split('/').next().unwrap_or(rest));
        if !authority.is_some_and(|a| is_allowed(host_name(a))) {
            return Some(forbidden("Origin", origin));
        }
    }
    None
}

/// 2025-11-25 basic/transports.mdx: an invalid or unsupported
/// `MCP-Protocol-Version` gets 400. A missing header is accepted.
fn protocol_version_rejection(headers: &HeaderMap) -> Option<Response> {
    let version = headers.get(PROTOCOL_VERSION_HEADER)?;
    let version = version.to_str().unwrap_or_default();
    if SUPPORTED_PROTOCOL_VERSIONS.contains(&version) {
        return None;
    }
    Some(error_reply(
        StatusCode::BAD_REQUEST,
        JsonRpcError::invalid_request(Some(format!(
            "Unsupported MCP-Protocol-Version: {version} (supported: {})",
            SUPPORTED_PROTOCOL_VERSIONS.join(", ")
        ))),
    ))
}

/// Resolves the session for a POST: an existing session, a new one for
/// `initialize`, or an error response.
async fn resolve_session(
    state: &HttpState,
    headers: &HeaderMap,
    message: &JsonRpcMessage,
) -> Result<(Option<String>, bool), Response> {
    if !state.config.enable_sessions {
        return Ok((None, false));
    }
    let header = headers.get(SESSION_HEADER).and_then(|v| v.to_str().ok());
    if let Some(sid) = header {
        let mut sessions = state.sessions.write().await;
        return match sessions.get_mut(sid) {
            Some(session) => {
                session.last_active = Instant::now();
                Ok((Some(sid.to_string()), false))
            }
            None => Err(error_reply(
                StatusCode::NOT_FOUND,
                JsonRpcError::invalid_request(Some("Unknown session".to_string())),
            )),
        };
    }
    if message.method() == Some("initialize") {
        let sid = uuid::Uuid::new_v4().to_string();
        state.sessions.write().await.insert(
            sid.clone(),
            SessionState {
                id: sid.clone(),
                created_at: Instant::now(),
                last_active: Instant::now(),
            },
        );
        info!(session_id = %sid, "Created new session");
        return Ok((Some(sid), true));
    }
    Err(error_reply(
        StatusCode::BAD_REQUEST,
        JsonRpcError::invalid_request(Some("Missing Mcp-Session-Id header".to_string())),
    ))
}

/// Handles POST requests to the MCP endpoint.
async fn handle_post(
    State(state): State<Arc<HttpState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    debug!("Received POST request");

    if let Some(resp) = host_or_origin_rejection(&state.config, &headers) {
        return resp;
    }

    let message = match std::str::from_utf8(&body)
        .map_err(|e| TransportError::Deserialization(format!("Invalid UTF-8: {e}")))
        .and_then(decode_jsonrpc_message)
    {
        Ok(message) => message,
        Err(TransportError::InvalidMessage(detail)) => {
            return error_reply(
                StatusCode::BAD_REQUEST,
                JsonRpcError::invalid_request(Some(detail)),
            )
        }
        Err(e) => {
            return error_reply(
                StatusCode::BAD_REQUEST,
                JsonRpcError::parse_error(Some(e.to_string())),
            )
        }
    };

    if let Some(resp) = protocol_version_rejection(&headers) {
        return resp;
    }

    let (session, created) = match resolve_session(&state, &headers, &message).await {
        Ok(resolved) => resolved,
        Err(resp) => return resp,
    };

    let mut response = match message {
        JsonRpcMessage::Request(request) => forward_request(&state, session.clone(), request).await,
        JsonRpcMessage::Notification(notification) => {
            let message = rewrite_cancellation(&state, session.as_ref(), notification).await;
            if state.incoming_tx.send(message).await.is_err() {
                return server_gone();
            }
            accepted()
        }
        // The server does not send requests, so client responses are
        // acknowledged and dropped.
        JsonRpcMessage::Response(_) | JsonRpcMessage::ErrorResponse(_) => accepted(),
    };

    if created {
        if let Some(value) = session.and_then(|sid| HeaderValue::from_str(&sid).ok()) {
            response.headers_mut().insert(SESSION_HEADER, value);
        }
    }
    response
}

fn server_gone() -> Response {
    warn!("MCP server is not receiving messages");
    error_reply(
        StatusCode::INTERNAL_SERVER_ERROR,
        JsonRpcError::internal_error(Some("Server unavailable".to_string())),
    )
}

/// Forwards a request to the server under a transport-assigned id and waits
/// for its response.
async fn forward_request(
    state: &HttpState,
    session: Option<String>,
    mut request: crate::protocol::types::JsonRpcRequest,
) -> Response {
    let client_id = std::mem::replace(
        &mut request.id,
        RequestId::String(format!(
            "http-{}",
            state.next_id.fetch_add(1, Ordering::Relaxed)
        )),
    );
    let server_id = request.id.to_string();
    let (respond, response_rx) = oneshot::channel();
    {
        let mut in_flight = state.in_flight.lock().await;
        in_flight
            .by_client_id
            .insert((session.clone(), client_id.to_string()), server_id.clone());
        in_flight.by_server_id.insert(
            server_id.clone(),
            PendingRequest {
                client_id: client_id.clone(),
                session: session.clone(),
                respond,
            },
        );
    }

    if state
        .incoming_tx
        .send(JsonRpcMessage::Request(request))
        .await
        .is_err()
    {
        forget(state, &server_id).await;
        return server_gone();
    }

    match tokio::time::timeout(RESPONSE_TIMEOUT, response_rx).await {
        Ok(Ok(response)) => json_reply(StatusCode::OK, &response),
        // The pending entry was dropped because the client cancelled the
        // request: a cancelled request gets no response.
        Ok(Err(_)) => (StatusCode::ACCEPTED, Body::empty()).into_response(),
        Err(_) => {
            warn!("Timeout waiting for server response");
            forget(state, &server_id).await;
            json_reply(
                StatusCode::GATEWAY_TIMEOUT,
                &JsonRpcMessage::error_response(
                    client_id,
                    JsonRpcError::internal_error(Some("Request timed out".to_string())),
                ),
            )
        }
    }
}

async fn forget(state: &HttpState, server_id: &str) {
    let mut in_flight = state.in_flight.lock().await;
    if let Some(p) = in_flight.by_server_id.remove(server_id) {
        in_flight
            .by_client_id
            .remove(&(p.session, p.client_id.to_string()));
    }
}

/// Points a client's `notifications/cancelled` at the transport-assigned id
/// the server knows, and releases the waiting POST.
async fn rewrite_cancellation(
    state: &HttpState,
    session: Option<&String>,
    mut notification: crate::protocol::types::JsonRpcNotification,
) -> JsonRpcMessage {
    if notification.method == "notifications/cancelled" {
        let client_id = notification
            .params
            .as_ref()
            .and_then(|p| p.get("requestId"))
            .and_then(|id| serde_json::from_value::<RequestId>(id.clone()).ok());
        if let Some(client_id) = client_id {
            let mut in_flight = state.in_flight.lock().await;
            if let Some(server_id) = in_flight
                .by_client_id
                .remove(&(session.cloned(), client_id.to_string()))
            {
                // Dropping the sender releases the POST without a body.
                in_flight.by_server_id.remove(&server_id);
                if let Some(params) = notification.params.as_mut() {
                    params["requestId"] = serde_json::Value::String(server_id);
                }
            }
        }
    }
    JsonRpcMessage::Notification(notification)
}

/// Handles GET requests: this transport offers no SSE stream, which the
/// specification answers with 405 Method Not Allowed.
async fn handle_get(State(state): State<Arc<HttpState>>, headers: HeaderMap) -> Response {
    if let Some(resp) = host_or_origin_rejection(&state.config, &headers) {
        return resp;
    }
    debug!("GET stream requested; not offered");
    (
        StatusCode::METHOD_NOT_ALLOWED,
        [(header::ALLOW, HeaderValue::from_static("POST, DELETE"))],
    )
        .into_response()
}

/// Handles DELETE requests for session termination.
async fn handle_delete(State(state): State<Arc<HttpState>>, headers: HeaderMap) -> Response {
    if let Some(resp) = host_or_origin_rejection(&state.config, &headers) {
        return resp;
    }
    let Some(sid) = headers.get(SESSION_HEADER).and_then(|v| v.to_str().ok()) else {
        warn!("DELETE request missing Mcp-Session-Id header");
        return StatusCode::BAD_REQUEST.into_response();
    };
    if state.sessions.write().await.remove(sid).is_some() {
        info!(session_id = %sid, "Session terminated");
        StatusCode::NO_CONTENT.into_response()
    } else {
        warn!(session_id = %sid, "Attempted to delete unknown session");
        StatusCode::NOT_FOUND.into_response()
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
        let transport = HttpTransport::builder().enable_sessions(false).build();
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
        // A request is forwarded under a transport-assigned id, and the
        // response comes back with the client's id restored.
        let mut transport = HttpTransport::builder().enable_sessions(false).build();
        let router = transport.router();

        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#))
            .unwrap();
        let call = tokio::spawn(router.oneshot(request));

        let forwarded = transport.recv().await.unwrap().expect("forwarded request");
        let server_id = forwarded.id().cloned().expect("request id");
        assert_ne!(server_id, RequestId::Number(1));
        transport
            .send(&JsonRpcMessage::response(
                server_id,
                serde_json::json!({"status": "ok"}),
            ))
            .await
            .unwrap();

        let response = call.await.unwrap().unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["result"]["status"], "ok");
        assert_eq!(json["id"], 1);
    }
}
