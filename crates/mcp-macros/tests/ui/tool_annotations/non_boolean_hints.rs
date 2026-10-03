#![allow(unused_imports)]

use mcp_macros::mcp_tool;
use mcp_sdk::types::content::CallToolResult;
use mcp_sdk::types::error::ToolError;

#[mcp_tool(read_only_hint = "true")]
async fn string_hint() -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text("hello"))
}

#[mcp_tool(destructive_hint = 1)]
async fn numeric_hint() -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text("hello"))
}

#[mcp_tool(idempotent_hint = enabled)]
async fn identifier_hint() -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text("hello"))
}

#[mcp_tool(open_world_hint = true || false)]
async fn expression_hint() -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text("hello"))
}

fn main() {}
