//! MCP Server builder and runtime.
//!
//! The [`McpServer`] is the main entry point for building and running an
//! MCP server. It provides a builder pattern for configuring the server,
//! registering tools, resources, and prompts, and starting the server
//! over a chosen transport.
//!
//! # Concurrency
//!
//! The server processes requests concurrently using `tokio::spawn`. Tool
//! calls, resource reads, and prompt gets are dispatched to separate tasks,
//! allowing multiple operations to execute in parallel. Non-blocking
//! operations like `ping` and `tools/list` respond immediately even while
//! long-running tool calls are in progress.
//!
//! # Example
//!
//! ```rust,no_run
//! use mcp_sdk::server::McpServer;
//! use mcp_sdk::transport::stdio::StdioTransport;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let server = McpServer::builder()
//!     .name("my-server")
//!     .version("1.0.0")
//!     .build();
//!
//! server.run(StdioTransport::new()).await?;
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::context::ToolContext;
use crate::handler::prompts::{PromptHandler, PromptRegistry};
use crate::handler::resources::{ResourceHandler, ResourceRegistry};
use crate::handler::tools::{ToolHandler, ToolRegistry};
use crate::protocol::capabilities::{
    ServerCapabilities, ServerCapabilitiesBuilder, PROTOCOL_VERSION,
};
use crate::protocol::messages::{
    methods, CallToolParams, GetPromptParams, GetPromptResult, Implementation, InitializeParams,
    InitializeResult, ListPromptsResult, ListResourceTemplatesResult, ListResourcesResult,
    ListToolsResult, PingResult, ReadResourceParams, ReadResourceResult, SetLevelParams,
};
use crate::protocol::types::{JsonRpcMessage, JsonRpcNotification, JsonRpcRequest, RequestId};
use crate::transport::McpTransport;
use crate::types::error::{JsonRpcError, McpError};

/// Supported protocol versions.
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2024-11-05"];

/// Builder for constructing an [`McpServer`].
///
/// # Example
///
/// ```rust
/// use mcp_sdk::server::McpServerBuilder;
///
/// let server = McpServerBuilder::new()
///     .name("calculator")
///     .version("1.0.0")
///     .build();
/// ```
pub struct McpServerBuilder {
    name: String,
    version: String,
    instructions: Option<String>,
    tools: Vec<Arc<dyn ToolHandler>>,
    resources: Vec<Arc<dyn ResourceHandler>>,
    prompts: Vec<Arc<dyn PromptHandler>>,
}

impl McpServerBuilder {
    /// Creates a new server builder with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            name: "mcp-server".to_string(),
            version: "0.1.0".to_string(),
            instructions: None,
            tools: Vec::new(),
            resources: Vec::new(),
            prompts: Vec::new(),
        }
    }

    /// Sets the server name.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Sets the server version.
    #[must_use]
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Sets optional instructions for the server.
    #[must_use]
    pub fn instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    /// Registers a tool handler.
    #[must_use]
    pub fn tool(mut self, handler: impl ToolHandler) -> Self {
        self.tools.push(Arc::new(handler));
        self
    }

    /// Registers a tool handler from an Arc.
    #[must_use]
    pub fn tool_arc(mut self, handler: Arc<dyn ToolHandler>) -> Self {
        self.tools.push(handler);
        self
    }

    /// Registers multiple tool handlers.
    #[must_use]
    pub fn tools(mut self, handlers: Vec<Arc<dyn ToolHandler>>) -> Self {
        self.tools.extend(handlers);
        self
    }

    /// Registers a resource handler.
    #[must_use]
    pub fn resource(mut self, handler: impl ResourceHandler) -> Self {
        self.resources.push(Arc::new(handler));
        self
    }

    /// Registers a resource handler from an Arc.
    #[must_use]
    pub fn resource_arc(mut self, handler: Arc<dyn ResourceHandler>) -> Self {
        self.resources.push(handler);
        self
    }

    /// Registers a prompt handler.
    #[must_use]
    pub fn prompt(mut self, handler: impl PromptHandler) -> Self {
        self.prompts.push(Arc::new(handler));
        self
    }

    /// Registers a prompt handler from an Arc.
    #[must_use]
    pub fn prompt_arc(mut self, handler: Arc<dyn PromptHandler>) -> Self {
        self.prompts.push(handler);
        self
    }

    /// Builds the [`McpServer`].
    ///
    /// # Panics
    ///
    /// Panics if duplicate tool, resource, or prompt names are registered.
    #[must_use]
    pub fn build(self) -> McpServer {
        let mut tool_registry = ToolRegistry::new();
        for handler in self.tools {
            tool_registry
                .register(handler)
                .expect("Failed to register tool (duplicate name?)");
        }

        let mut resource_registry = ResourceRegistry::new();
        for handler in self.resources {
            resource_registry
                .register(handler)
                .expect("Failed to register resource (duplicate URI?)");
        }

        let mut prompt_registry = PromptRegistry::new();
        for handler in self.prompts {
            prompt_registry
                .register(handler)
                .expect("Failed to register prompt (duplicate name?)");
        }

        // Build capabilities based on what's registered
        let mut caps_builder = ServerCapabilitiesBuilder::new();
        if !tool_registry.is_empty() {
            caps_builder = caps_builder.enable_tools(false);
        }
        if !resource_registry.is_empty() {
            caps_builder = caps_builder.enable_resources(false);
        }
        if !prompt_registry.is_empty() {
            caps_builder = caps_builder.enable_prompts(false);
        }

        let state = Arc::new(ServerState {
            name: self.name,
            version: self.version,
            instructions: self.instructions,
            capabilities: caps_builder.build(),
            tool_registry,
            resource_registry,
            prompt_registry,
            initialized: AtomicBool::new(false),
            pending_cancellations: Mutex::new(HashMap::new()),
        });

        McpServer { state }
    }
}

impl Default for McpServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared server state that is safely accessible from multiple concurrent tasks.
///
/// This is wrapped in an `Arc` so that spawned tasks (for concurrent tool calls)
/// can access the registries and state without holding a mutable borrow on the server.
pub struct ServerState {
    name: String,
    version: String,
    instructions: Option<String>,
    capabilities: ServerCapabilities,
    tool_registry: ToolRegistry,
    resource_registry: ResourceRegistry,
    prompt_registry: PromptRegistry,
    /// Whether the server has completed the initialization handshake.
    initialized: AtomicBool,
    /// Cancellation tokens for pending requests, keyed by request ID string.
    pending_cancellations: Mutex<HashMap<String, CancellationToken>>,
}

impl ServerState {
    /// Registers a cancellation token for a pending request.
    async fn register_cancellation(&self, request_id: &str) -> CancellationToken {
        let token = CancellationToken::new();
        let mut pending = self.pending_cancellations.lock().await;
        pending.insert(request_id.to_string(), token.clone());
        token
    }

    /// Removes and returns the cancellation token for a completed request.
    async fn unregister_cancellation(&self, request_id: &str) {
        let mut pending = self.pending_cancellations.lock().await;
        pending.remove(request_id);
    }

    /// Cancels a pending request by its ID.
    async fn cancel_request(&self, request_id: &str) {
        let pending = self.pending_cancellations.lock().await;
        if let Some(token) = pending.get(request_id) {
            debug!(request_id = %request_id, "Cancelling request");
            token.cancel();
        }
    }
}

/// An MCP server that handles JSON-RPC requests over a transport.
///
/// The server manages the lifecycle of an MCP connection, including
/// the initialization handshake, tool/resource/prompt dispatch, and
/// graceful shutdown. Requests are processed concurrently using
/// `tokio::spawn`.
pub struct McpServer {
    state: Arc<ServerState>,
}

impl McpServer {
    /// Returns a new [`McpServerBuilder`].
    #[must_use]
    pub fn builder() -> McpServerBuilder {
        McpServerBuilder::new()
    }

    /// Runs the server over the given transport until the connection is closed.
    ///
    /// This method enters the main message loop, receiving JSON-RPC messages
    /// from the transport, dispatching them to the appropriate handler, and
    /// sending responses back. Tool calls, resource reads, and prompt gets
    /// are dispatched concurrently via `tokio::spawn`.
    ///
    /// # Errors
    ///
    /// Returns an error if a fatal transport error occurs.
    pub async fn run(self, mut transport: impl McpTransport) -> Result<(), McpError> {
        info!(
            server_name = %self.state.name,
            server_version = %self.state.version,
            tool_count = self.state.tool_registry.len(),
            resource_count = self.state.resource_registry.len(),
            prompt_count = self.state.prompt_registry.len(),
            "MCP server starting"
        );

        loop {
            let message = match transport.recv().await {
                Ok(Some(msg)) => msg,
                Ok(None) => {
                    info!("Transport closed, shutting down");
                    break;
                }
                Err(e) => {
                    // For deserialization errors, send a parse error response
                    // and continue. For I/O errors, shut down.
                    if let crate::types::error::TransportError::Deserialization(detail) = &e {
                        warn!(error = %detail, "Failed to parse incoming message");
                        let error_response = JsonRpcMessage::error_response(
                            RequestId::Number(0),
                            JsonRpcError::parse_error(Some(detail.clone())),
                        );
                        if let Err(send_err) = transport.send(&error_response).await {
                            error!(error = %send_err, "Failed to send parse error response");
                        }
                        continue;
                    }
                    error!(error = %e, "Fatal transport error");
                    return Err(McpError::Transport(e));
                }
            };

            match message {
                JsonRpcMessage::Request(request) => {
                    let state = Arc::clone(&self.state);
                    let method = request.method.clone();

                    // For async operations (tool calls, resource reads, prompt gets),
                    // dispatch concurrently. For synchronous operations, handle inline
                    // to avoid unnecessary task spawning overhead.
                    match method.as_str() {
                        methods::TOOLS_CALL | methods::RESOURCES_READ | methods::PROMPTS_GET => {
                            // Register cancellation token for this request
                            let request_id_str = request.id.to_string();
                            let cancel_token = state.register_cancellation(&request_id_str).await;

                            let response = {
                                let state = Arc::clone(&state);
                                tokio::spawn(async move {
                                    let result =
                                        handle_async_request(&state, request, cancel_token).await;
                                    state.unregister_cancellation(&request_id_str).await;
                                    result
                                })
                            };

                            // Wait for the spawned task and send response
                            match response.await {
                                Ok(resp) => {
                                    if let Err(e) = transport.send(&resp).await {
                                        error!(error = %e, "Failed to send response");
                                        return Err(McpError::Transport(e));
                                    }
                                }
                                Err(e) => {
                                    error!(error = %e, "Spawned task panicked");
                                    let error_resp = JsonRpcMessage::error_response(
                                        RequestId::Number(0),
                                        JsonRpcError::internal_error(Some(
                                            "Internal task error".to_string(),
                                        )),
                                    );
                                    if let Err(e) = transport.send(&error_resp).await {
                                        error!(error = %e, "Failed to send error response");
                                        return Err(McpError::Transport(e));
                                    }
                                }
                            }
                        }
                        _ => {
                            // Handle synchronous requests inline
                            let response = handle_sync_request(&state, &request);
                            if let Err(e) = transport.send(&response).await {
                                error!(error = %e, "Failed to send response");
                                return Err(McpError::Transport(e));
                            }
                        }
                    }
                }
                JsonRpcMessage::Notification(notification) => {
                    handle_notification(&self.state, &notification).await;
                }
                JsonRpcMessage::Response(_) | JsonRpcMessage::ErrorResponse(_) => {
                    // Servers don't expect responses from clients in normal flow.
                    debug!("Received unexpected response/error from client, ignoring");
                }
            }
        }

        transport.close().await.map_err(McpError::Transport)?;
        info!("MCP server shut down gracefully");
        Ok(())
    }
}

/// Handles synchronous requests (initialize, ping, list operations).
fn handle_sync_request(state: &ServerState, request: &JsonRpcRequest) -> JsonRpcMessage {
    let id = request.id.clone();

    debug!(method = %request.method, id = %id, "Handling sync request");

    // Before initialization, only `initialize` is allowed
    if !state.initialized.load(Ordering::Acquire) && request.method != methods::INITIALIZE {
        return JsonRpcMessage::error_response(
            id,
            JsonRpcError::invalid_request(Some(
                "Server not initialized. Send 'initialize' first.".to_string(),
            )),
        );
    }

    match request.method.as_str() {
        methods::INITIALIZE => handle_initialize(state, &id, request.params.as_ref()),
        methods::PING => handle_ping(&id),
        methods::TOOLS_LIST => handle_tools_list(state, &id),
        methods::RESOURCES_LIST => handle_resources_list(state, &id),
        methods::RESOURCES_TEMPLATES_LIST => handle_resource_templates_list(state, &id),
        methods::PROMPTS_LIST => handle_prompts_list(state, &id),
        methods::LOGGING_SET_LEVEL => handle_set_log_level(&id, request.params.as_ref()),
        _ => {
            warn!(method = %request.method, "Unknown method");
            JsonRpcMessage::error_response(
                id,
                JsonRpcError::method_not_found(Some(request.method.clone())),
            )
        }
    }
}

/// Handles async requests (tool calls, resource reads, prompt gets).
async fn handle_async_request(
    state: &ServerState,
    request: JsonRpcRequest,
    cancel_token: CancellationToken,
) -> JsonRpcMessage {
    let id = request.id.clone();

    debug!(method = %request.method, id = %id, "Handling async request");

    // Before initialization, only `initialize` is allowed
    if !state.initialized.load(Ordering::Acquire) {
        return JsonRpcMessage::error_response(
            id,
            JsonRpcError::invalid_request(Some(
                "Server not initialized. Send 'initialize' first.".to_string(),
            )),
        );
    }

    match request.method.as_str() {
        methods::TOOLS_CALL => {
            handle_tools_call(state, &id, request.params.as_ref(), cancel_token).await
        }
        methods::RESOURCES_READ => handle_resources_read(state, &id, request.params.as_ref()).await,
        methods::PROMPTS_GET => handle_prompts_get(state, &id, request.params.as_ref()).await,
        _ => {
            warn!(method = %request.method, "Unknown async method");
            JsonRpcMessage::error_response(
                id,
                JsonRpcError::method_not_found(Some(request.method.clone())),
            )
        }
    }
}

/// Handles notifications from the client.
async fn handle_notification(state: &ServerState, notification: &JsonRpcNotification) {
    debug!(method = %notification.method, "Handling notification");

    match notification.method.as_str() {
        methods::INITIALIZED => {
            info!("Client confirmed initialization");
        }
        methods::NOTIFICATION_CANCELLED => {
            debug!("Received cancellation notification");
            // Extract request ID from params and cancel the pending request
            if let Some(params) = &notification.params {
                if let Some(request_id) = params.get("requestId") {
                    let id_str = match request_id {
                        serde_json::Value::Number(n) => n.to_string(),
                        serde_json::Value::String(s) => s.clone(),
                        _ => return,
                    };
                    state.cancel_request(&id_str).await;
                }
            }
        }
        _ => {
            debug!(method = %notification.method, "Ignoring unknown notification");
        }
    }
}

/// Handles the `initialize` request.
fn handle_initialize(
    state: &ServerState,
    id: &RequestId,
    params: Option<&serde_json::Value>,
) -> JsonRpcMessage {
    let init_params: InitializeParams = match params {
        Some(p) => match serde_json::from_value(p.clone()) {
            Ok(params) => params,
            Err(e) => {
                return JsonRpcMessage::error_response(
                    id.clone(),
                    JsonRpcError::invalid_params(Some(format!("Invalid initialize params: {e}"))),
                );
            }
        },
        None => {
            return JsonRpcMessage::error_response(
                id.clone(),
                JsonRpcError::invalid_params(Some("Missing initialize params".to_string())),
            );
        }
    };

    info!(
        client = %init_params.client_info.name,
        client_version = %init_params.client_info.version,
        protocol_version = %init_params.protocol_version,
        "Initialize request received"
    );

    // Validate protocol version compatibility
    let negotiated_version =
        if SUPPORTED_PROTOCOL_VERSIONS.contains(&init_params.protocol_version.as_str()) {
            init_params.protocol_version.clone()
        } else {
            warn!(
                requested_version = %init_params.protocol_version,
                supported_versions = ?SUPPORTED_PROTOCOL_VERSIONS,
                "Client requested unsupported protocol version, using latest"
            );
            // Use the latest supported version
            PROTOCOL_VERSION.to_string()
        };

    state.initialized.store(true, Ordering::Release);

    let result = InitializeResult {
        protocol_version: negotiated_version,
        capabilities: state.capabilities.clone(),
        server_info: Implementation {
            name: state.name.clone(),
            version: state.version.clone(),
        },
        instructions: state.instructions.clone(),
    };

    match serde_json::to_value(&result) {
        Ok(value) => JsonRpcMessage::response(id.clone(), value),
        Err(e) => JsonRpcMessage::error_response(
            id.clone(),
            JsonRpcError::internal_error(Some(format!("Failed to serialize result: {e}"))),
        ),
    }
}

/// Handles the `ping` request.
fn handle_ping(id: &RequestId) -> JsonRpcMessage {
    let result = PingResult {};
    match serde_json::to_value(&result) {
        Ok(value) => JsonRpcMessage::response(id.clone(), value),
        Err(e) => JsonRpcMessage::error_response(
            id.clone(),
            JsonRpcError::internal_error(Some(format!("Failed to serialize ping: {e}"))),
        ),
    }
}

/// Handles the `tools/list` request.
fn handle_tools_list(state: &ServerState, id: &RequestId) -> JsonRpcMessage {
    let tools = state.tool_registry.list();
    let result = ListToolsResult {
        tools,
        next_cursor: None,
    };
    match serde_json::to_value(&result) {
        Ok(value) => JsonRpcMessage::response(id.clone(), value),
        Err(e) => JsonRpcMessage::error_response(
            id.clone(),
            JsonRpcError::internal_error(Some(format!("Failed to serialize tools list: {e}"))),
        ),
    }
}

/// Handles the `tools/call` request with cancellation support.
async fn handle_tools_call(
    state: &ServerState,
    id: &RequestId,
    params: Option<&serde_json::Value>,
    cancel_token: CancellationToken,
) -> JsonRpcMessage {
    let call_params: CallToolParams = match params {
        Some(p) => match serde_json::from_value(p.clone()) {
            Ok(params) => params,
            Err(e) => {
                return JsonRpcMessage::error_response(
                    id.clone(),
                    JsonRpcError::invalid_params(Some(format!("Invalid tools/call params: {e}"))),
                );
            }
        },
        None => {
            return JsonRpcMessage::error_response(
                id.clone(),
                JsonRpcError::invalid_params(Some("Missing tools/call params".to_string())),
            );
        }
    };

    let context = ToolContext::with_cancellation(id.to_string(), cancel_token.clone());

    // Race the tool call against cancellation
    tokio::select! {
        result = state.tool_registry.call(&call_params.name, call_params.arguments, &context) => {
            match result {
                Ok(result) => match serde_json::to_value(&result) {
                    Ok(value) => JsonRpcMessage::response(id.clone(), value),
                    Err(e) => JsonRpcMessage::error_response(
                        id.clone(),
                        JsonRpcError::internal_error(Some(format!(
                            "Failed to serialize tool result: {e}"
                        ))),
                    ),
                },
                Err(e) => JsonRpcMessage::error_response(
                    id.clone(),
                    JsonRpcError::new(e.error_code(), Some(e.to_string())),
                ),
            }
        }
        () = cancel_token.cancelled() => {
            warn!(id = %id, "Tool call cancelled");
            JsonRpcMessage::error_response(
                id.clone(),
                JsonRpcError::internal_error(Some("Request cancelled".to_string())),
            )
        }
    }
}

/// Handles the `resources/list` request.
fn handle_resources_list(state: &ServerState, id: &RequestId) -> JsonRpcMessage {
    let resources = state.resource_registry.list();
    let result = ListResourcesResult {
        resources,
        next_cursor: None,
    };
    match serde_json::to_value(&result) {
        Ok(value) => JsonRpcMessage::response(id.clone(), value),
        Err(e) => JsonRpcMessage::error_response(
            id.clone(),
            JsonRpcError::internal_error(Some(format!("Failed to serialize resources list: {e}"))),
        ),
    }
}

/// Handles the `resources/templates/list` request.
fn handle_resource_templates_list(state: &ServerState, id: &RequestId) -> JsonRpcMessage {
    let templates = state.resource_registry.list_templates();
    let result = ListResourceTemplatesResult {
        resource_templates: templates,
        next_cursor: None,
    };
    match serde_json::to_value(&result) {
        Ok(value) => JsonRpcMessage::response(id.clone(), value),
        Err(e) => JsonRpcMessage::error_response(
            id.clone(),
            JsonRpcError::internal_error(Some(format!(
                "Failed to serialize resource templates: {e}"
            ))),
        ),
    }
}

/// Handles the `resources/read` request.
async fn handle_resources_read(
    state: &ServerState,
    id: &RequestId,
    params: Option<&serde_json::Value>,
) -> JsonRpcMessage {
    let read_params: ReadResourceParams = match params {
        Some(p) => match serde_json::from_value(p.clone()) {
            Ok(params) => params,
            Err(e) => {
                return JsonRpcMessage::error_response(
                    id.clone(),
                    JsonRpcError::invalid_params(Some(format!(
                        "Invalid resources/read params: {e}"
                    ))),
                );
            }
        },
        None => {
            return JsonRpcMessage::error_response(
                id.clone(),
                JsonRpcError::invalid_params(Some("Missing resources/read params".to_string())),
            );
        }
    };

    match state.resource_registry.read(&read_params.uri).await {
        Ok(content) => {
            let result = ReadResourceResult {
                contents: vec![content],
            };
            match serde_json::to_value(&result) {
                Ok(value) => JsonRpcMessage::response(id.clone(), value),
                Err(e) => JsonRpcMessage::error_response(
                    id.clone(),
                    JsonRpcError::internal_error(Some(format!(
                        "Failed to serialize resource: {e}"
                    ))),
                ),
            }
        }
        Err(e) => JsonRpcMessage::error_response(
            id.clone(),
            JsonRpcError::new(e.error_code(), Some(e.to_string())),
        ),
    }
}

/// Handles the `prompts/list` request.
fn handle_prompts_list(state: &ServerState, id: &RequestId) -> JsonRpcMessage {
    let prompts = state.prompt_registry.list();
    let result = ListPromptsResult {
        prompts,
        next_cursor: None,
    };
    match serde_json::to_value(&result) {
        Ok(value) => JsonRpcMessage::response(id.clone(), value),
        Err(e) => JsonRpcMessage::error_response(
            id.clone(),
            JsonRpcError::internal_error(Some(format!("Failed to serialize prompts list: {e}"))),
        ),
    }
}

/// Handles the `prompts/get` request.
async fn handle_prompts_get(
    state: &ServerState,
    id: &RequestId,
    params: Option<&serde_json::Value>,
) -> JsonRpcMessage {
    let get_params: GetPromptParams = match params {
        Some(p) => match serde_json::from_value(p.clone()) {
            Ok(params) => params,
            Err(e) => {
                return JsonRpcMessage::error_response(
                    id.clone(),
                    JsonRpcError::invalid_params(Some(format!("Invalid prompts/get params: {e}"))),
                );
            }
        },
        None => {
            return JsonRpcMessage::error_response(
                id.clone(),
                JsonRpcError::invalid_params(Some("Missing prompts/get params".to_string())),
            );
        }
    };

    match state
        .prompt_registry
        .get(&get_params.name, get_params.arguments)
        .await
    {
        Ok(messages) => {
            let result = GetPromptResult {
                description: None,
                messages,
            };
            match serde_json::to_value(&result) {
                Ok(value) => JsonRpcMessage::response(id.clone(), value),
                Err(e) => JsonRpcMessage::error_response(
                    id.clone(),
                    JsonRpcError::internal_error(Some(format!("Failed to serialize prompt: {e}"))),
                ),
            }
        }
        Err(e) => JsonRpcMessage::error_response(
            id.clone(),
            JsonRpcError::new(e.error_code(), Some(e.to_string())),
        ),
    }
}

/// Handles the `logging/setLevel` request.
fn handle_set_log_level(id: &RequestId, params: Option<&serde_json::Value>) -> JsonRpcMessage {
    let level_params: SetLevelParams = match params {
        Some(p) => match serde_json::from_value(p.clone()) {
            Ok(params) => params,
            Err(e) => {
                return JsonRpcMessage::error_response(
                    id.clone(),
                    JsonRpcError::invalid_params(Some(format!(
                        "Invalid logging/setLevel params: {e}"
                    ))),
                );
            }
        },
        None => {
            return JsonRpcMessage::error_response(
                id.clone(),
                JsonRpcError::invalid_params(Some("Missing logging/setLevel params".to_string())),
            );
        }
    };

    info!(level = ?level_params.level, "Log level changed");

    // Acknowledge the level change
    JsonRpcMessage::response(id.clone(), serde_json::json!({}))
}

impl std::fmt::Debug for McpServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpServer")
            .field("name", &self.state.name)
            .field("version", &self.state.version)
            .field("instructions", &self.state.instructions)
            .field("capabilities", &self.state.capabilities)
            .field(
                "initialized",
                &self.state.initialized.load(Ordering::Relaxed),
            )
            .field("tool_registry", &self.state.tool_registry)
            .field("resource_registry", &self.state.resource_registry)
            .field("prompt_registry", &self.state.prompt_registry)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::tools::ToolHandler;
    use crate::protocol::messages::ToolInfo;
    use crate::types::content::CallToolResult;

    struct EchoTool;

    #[async_trait::async_trait]
    impl ToolHandler for EchoTool {
        fn info(&self) -> ToolInfo {
            ToolInfo {
                name: "echo".to_string(),
                description: Some("Echo the input".to_string()),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "message": {"type": "string"}
                    },
                    "required": ["message"]
                }),
            }
        }

        async fn call(
            &self,
            arguments: serde_json::Value,
            _context: &ToolContext,
        ) -> Result<CallToolResult, crate::types::error::ToolError> {
            let msg = arguments
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("(empty)");
            Ok(CallToolResult::text(msg))
        }
    }

    #[test]
    fn test_builder_defaults() {
        let server = McpServer::builder().build();
        assert_eq!(server.state.name, "mcp-server");
        assert!(!server.state.initialized.load(Ordering::Relaxed));
    }

    #[test]
    fn test_builder_with_tools() {
        let server = McpServer::builder()
            .name("test")
            .version("1.0")
            .tool(EchoTool)
            .build();

        assert_eq!(server.state.name, "test");
        assert_eq!(server.state.version, "1.0");
        assert_eq!(server.state.tool_registry.len(), 1);
        assert!(server.state.capabilities.tools.is_some());
    }

    #[test]
    fn test_capabilities_reflect_registrations() {
        let server = McpServer::builder().build();
        assert!(server.state.capabilities.tools.is_none());
        assert!(server.state.capabilities.resources.is_none());
        assert!(server.state.capabilities.prompts.is_none());

        let server = McpServer::builder().tool(EchoTool).build();
        assert!(server.state.capabilities.tools.is_some());
    }

    #[test]
    fn test_protocol_version_validation() {
        let state = McpServer::builder().build().state;

        // Supported version should be accepted
        let params = serde_json::json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}
        });
        let response = handle_initialize(&state, &RequestId::Number(1), Some(&params));
        if let JsonRpcMessage::Response(resp) = response {
            assert_eq!(resp.result["protocolVersion"], "2025-11-25");
        } else {
            panic!("Expected response");
        }

        // Reset initialized for next test
        state.initialized.store(false, Ordering::Release);

        // Unsupported version should negotiate to latest
        let params = serde_json::json!({
            "protocolVersion": "9999-01-01",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}
        });
        let response = handle_initialize(&state, &RequestId::Number(2), Some(&params));
        if let JsonRpcMessage::Response(resp) = response {
            assert_eq!(resp.result["protocolVersion"], PROTOCOL_VERSION);
        } else {
            panic!("Expected response");
        }
    }
}
