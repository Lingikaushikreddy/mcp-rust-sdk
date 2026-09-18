//! Convenience re-exports for common MCP SDK types.
//!
//! Import this module to bring the most commonly used items into scope:
//!
//! ```rust
//! use mcp_sdk::prelude::*;
//! ```

pub use crate::context::ToolContext;
pub use crate::handler::prompts::{PromptHandler, PromptRegistry};
pub use crate::handler::resources::{ResourceHandler, ResourceRegistry};
pub use crate::handler::tools::{ToolHandler, ToolRegistry};
pub use crate::protocol::capabilities::{ServerCapabilities, ServerCapabilitiesBuilder};
pub use crate::protocol::messages::{
    CallToolParams, GetPromptParams, GetPromptResult, Implementation, InitializeParams,
    InitializeResult, ListToolsResult, PromptArgument, PromptInfo, ResourceInfo,
    ResourceTemplateInfo, ToolInfo,
};
pub use crate::protocol::types::{JsonRpcMessage, RequestId};
pub use crate::schema::JsonSchemaBuilder;
pub use crate::server::{McpServer, McpServerBuilder};
pub use crate::transport::stdio::StdioTransport;
pub use crate::transport::McpTransport;
pub use crate::types::content::CallToolResult;
pub use crate::types::content::{Content, PromptMessage, ResourceContent};
pub use crate::types::error::{McpError, PromptError, ResourceError, ToolError, TransportError};

// Re-export proc macros when the feature is enabled
#[cfg(feature = "macros")]
pub use mcp_macros::{mcp_prompt, mcp_resource, mcp_tool};

// Re-export async_trait for convenience
pub use async_trait::async_trait;
