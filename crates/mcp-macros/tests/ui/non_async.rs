use mcp_macros::mcp_tool;
use mcp_sdk::types::content::CallToolResult;
use mcp_sdk::types::error::ToolError;

#[mcp_tool]
fn not_async() -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text("hello"))
}

fn main() {}
