#![allow(unused_imports)]

use mcp_macros::mcp_tool;
use mcp_sdk::types::content::CallToolResult;
use mcp_sdk::types::error::ToolError;

#[mcp_tool(read_only_hint = true, read_only_hint = false)]
async fn duplicate_hint() -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text("hello"))
}

#[mcp_tool(title = "First", title = "Second")]
async fn duplicate_title() -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text("hello"))
}

#[mcp_tool(name = "first", name = "second")]
async fn duplicate_name() -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text("hello"))
}

#[mcp_tool(description = "First", description = "Second")]
async fn duplicate_description() -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text("hello"))
}

#[mcp_tool(destructive, destructive)]
async fn duplicate_destructive() -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text("hello"))
}

fn main() {}
