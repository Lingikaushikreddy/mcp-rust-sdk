#![allow(unused_imports)]

use mcp_macros::mcp_tool;
use mcp_sdk::types::content::CallToolResult;
use mcp_sdk::types::error::ToolError;

#[mcp_tool(destructive, destructive_hint = false)]
async fn legacy_then_explicit() -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text("hello"))
}

#[mcp_tool(destructive_hint = false, destructive)]
async fn explicit_then_legacy() -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text("hello"))
}

fn main() {}
