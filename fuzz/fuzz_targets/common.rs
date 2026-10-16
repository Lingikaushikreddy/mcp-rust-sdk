//! Shared fuzz fixture: a server with one echo tool.

use async_trait::async_trait;
use mcp_sdk::context::ToolContext;
use mcp_sdk::handler::tools::ToolHandler;
use mcp_sdk::protocol::messages::ToolInfo;
use mcp_sdk::server::McpServer;
use mcp_sdk::types::content::CallToolResult;
use mcp_sdk::types::error::ToolError;
use serde_json::{json, Value};

struct Echo;

#[async_trait]
impl ToolHandler for Echo {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "echo".to_string(),
            description: None,
            input_schema: json!({"type": "object"}),
            annotations: None,
        }
    }

    async fn call(&self, arguments: Value, _context: &ToolContext) -> Result<CallToolResult, ToolError> {
        Ok(CallToolResult::text(arguments.to_string()))
    }
}

pub fn server() -> McpServer {
    McpServer::builder().name("fuzz").tool(Echo).build()
}
