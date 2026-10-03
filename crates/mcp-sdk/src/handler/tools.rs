//! Tool handler trait and registry.
//!
//! Tools are the primary way MCP servers expose functionality to AI clients.
//! Each tool has a name, description, JSON Schema for its input parameters,
//! and an async handler function that executes the tool.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tracing::{debug, warn};

use crate::context::ToolContext;
use crate::protocol::messages::ToolInfo;
use crate::types::content::CallToolResult;
use crate::types::error::ToolError;

/// Trait that must be implemented by all MCP tool handlers.
///
/// Implementations of this trait provide the tool metadata and the
/// execution logic for handling `tools/call` requests.
///
/// # Example
///
/// ```rust
/// use async_trait::async_trait;
/// use mcp_sdk::handler::ToolHandler;
/// use mcp_sdk::protocol::messages::ToolInfo;
/// use mcp_sdk::types::content::CallToolResult;
/// use mcp_sdk::types::error::ToolError;
/// use mcp_sdk::context::ToolContext;
///
/// struct EchoTool;
///
/// #[async_trait]
/// impl ToolHandler for EchoTool {
///     fn info(&self) -> ToolInfo {
///         ToolInfo {
///             annotations: Some(mcp_sdk::ToolAnnotations {
///                 read_only_hint: Some(true),
///                 destructive_hint: Some(false),
///                 idempotent_hint: Some(true),
///                 open_world_hint: Some(false),
///                 ..Default::default()
///             }),
///             name: "echo".to_string(),
///             description: Some("Echo back the input".to_string()),
///             input_schema: serde_json::json!({
///                 "type": "object",
///                 "properties": {
///                     "message": {"type": "string"}
///                 },
///                 "required": ["message"]
///             }),
///         }
///     }
///
///     async fn call(
///         &self,
///         arguments: serde_json::Value,
///         _context: &ToolContext,
///     ) -> Result<CallToolResult, ToolError> {
///         let message = arguments.get("message")
///             .and_then(|v| v.as_str())
///             .unwrap_or("(no message)");
///         Ok(CallToolResult::text(message))
///     }
/// }
/// ```
#[async_trait]
pub trait ToolHandler: Send + Sync + 'static {
    /// Returns the metadata for this tool.
    fn info(&self) -> ToolInfo;

    /// Executes the tool with the given arguments and context.
    async fn call(
        &self,
        arguments: Value,
        context: &ToolContext,
    ) -> Result<CallToolResult, ToolError>;
}

/// A tool handler built from a closure.
///
/// This allows registering tools without implementing the trait explicitly.
pub struct FnToolHandler<F>
where
    F: Fn(
            Value,
            &ToolContext,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<CallToolResult, ToolError>> + Send + '_>,
        > + Send
        + Sync
        + 'static,
{
    info: ToolInfo,
    handler: F,
}

impl<F> FnToolHandler<F>
where
    F: Fn(
            Value,
            &ToolContext,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<CallToolResult, ToolError>> + Send + '_>,
        > + Send
        + Sync
        + 'static,
{
    /// Creates a new function-based tool handler.
    pub fn new(info: ToolInfo, handler: F) -> Self {
        Self { info, handler }
    }
}

#[async_trait]
impl<F> ToolHandler for FnToolHandler<F>
where
    F: Fn(
            Value,
            &ToolContext,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<CallToolResult, ToolError>> + Send + '_>,
        > + Send
        + Sync
        + 'static,
{
    fn info(&self) -> ToolInfo {
        self.info.clone()
    }

    async fn call(
        &self,
        arguments: Value,
        context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        (self.handler)(arguments, context).await
    }
}

/// A registered tool entry containing both the handler and cached metadata.
struct ToolEntry {
    handler: Arc<dyn ToolHandler>,
    #[allow(dead_code)]
    info: ToolInfo,
}

/// Registry that holds all registered tool handlers and dispatches calls.
///
/// Tool metadata (`ToolInfo`) is cached at registration time to avoid
/// repeated allocations on every `tools/list` request.
#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, ToolEntry>,
    /// Cached list of tool infos, built at registration time.
    cached_infos: Vec<ToolInfo>,
}

impl ToolRegistry {
    /// Creates a new empty tool registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            cached_infos: Vec::new(),
        }
    }

    /// Registers a tool handler. Returns an error if a tool with the same name exists.
    ///
    /// The tool's metadata is cached at registration time so that `list()`
    /// does not need to call `handler.info()` on every request.
    ///
    /// # Errors
    ///
    /// Returns an error string if a tool with the given name is already registered.
    pub fn register(&mut self, handler: Arc<dyn ToolHandler>) -> Result<(), String> {
        let info = handler.info();
        let name = info.name.clone();
        if self.tools.contains_key(&name) {
            return Err(format!("Duplicate tool name: {name}"));
        }
        debug!(tool_name = %name, "Registered tool");
        self.cached_infos.push(info.clone());
        self.tools.insert(name, ToolEntry { handler, info });
        Ok(())
    }

    /// Returns cached metadata for all registered tools.
    ///
    /// This returns clones of the cached `ToolInfo` values without
    /// calling `handler.info()`, avoiding repeated allocations.
    #[must_use]
    pub fn list(&self) -> Vec<ToolInfo> {
        self.cached_infos.clone()
    }

    /// Returns the number of registered tools.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Returns true if no tools are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Returns a reference to the tool handler for the given name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Arc<dyn ToolHandler>> {
        self.tools.get(name).map(|e| &e.handler)
    }

    /// Calls a tool by name with the given arguments and context.
    ///
    /// # Errors
    ///
    /// Returns a `ToolError` if the tool is not found or if the handler fails.
    pub async fn call(
        &self,
        name: &str,
        arguments: Value,
        context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        let entry = self.tools.get(name).ok_or_else(|| {
            warn!(tool_name = %name, "Unknown tool requested");
            ToolError::InvalidParams(format!("Unknown tool: {name}"))
        })?;

        debug!(tool_name = %name, "Calling tool");
        entry.handler.call(arguments, context).await
    }
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistry")
            .field("tool_count", &self.tools.len())
            .field("tools", &self.tools.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AddTool;

    #[async_trait]
    impl ToolHandler for AddTool {
        fn info(&self) -> ToolInfo {
            ToolInfo {
                annotations: Some(crate::ToolAnnotations {
                    read_only_hint: Some(true),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                    ..Default::default()
                }),
                name: "add".to_string(),
                description: Some("Add two numbers".to_string()),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "a": {"type": "number"},
                        "b": {"type": "number"}
                    },
                    "required": ["a", "b"]
                }),
            }
        }

        async fn call(
            &self,
            arguments: Value,
            _context: &ToolContext,
        ) -> Result<CallToolResult, ToolError> {
            let a = arguments
                .get("a")
                .and_then(serde_json::Value::as_f64)
                .ok_or_else(|| ToolError::InvalidParams("Missing 'a'".to_string()))?;
            let b = arguments
                .get("b")
                .and_then(serde_json::Value::as_f64)
                .ok_or_else(|| ToolError::InvalidParams("Missing 'b'".to_string()))?;
            Ok(CallToolResult::text((a + b).to_string()))
        }
    }

    #[test]
    fn test_register_tool() {
        let mut registry = ToolRegistry::new();
        let result = registry.register(Arc::new(AddTool));
        assert!(result.is_ok());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_duplicate_tool() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(AddTool)).expect("first");
        let result = registry.register(Arc::new(AddTool));
        assert!(result.is_err());
    }

    #[test]
    fn test_list_tools() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(AddTool)).expect("register");
        let tools = registry.list();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "add");
    }

    #[test]
    fn test_list_uses_cached_info() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(AddTool)).expect("register");
        // Call list multiple times -- should return cached data
        let tools1 = registry.list();
        let tools2 = registry.list();
        assert_eq!(tools1, tools2);
        assert_eq!(tools1[0].name, "add");
    }

    #[tokio::test]
    async fn test_call_tool() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(AddTool)).expect("register");
        let ctx = ToolContext::default();
        let result = registry
            .call("add", serde_json::json!({"a": 1.0, "b": 2.0}), &ctx)
            .await
            .expect("call");
        assert_eq!(result.content[0], crate::types::content::Content::text("3"));
    }

    #[tokio::test]
    async fn test_call_unknown_tool() {
        let registry = ToolRegistry::new();
        let ctx = ToolContext::default();
        let result = registry
            .call("nonexistent", serde_json::json!({}), &ctx)
            .await;
        assert!(result.is_err());
    }
}
