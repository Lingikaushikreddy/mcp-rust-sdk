#![allow(unused_imports)]

use mcp_macros::mcp_tool;
use mcp_sdk::types::content::CallToolResult;
use mcp_sdk::types::error::ToolError;

#[mcp_tool(read_only_hint)]
async fn missing_hint_value() -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text("hello"))
}

#[mcp_tool(destructive = true)]
async fn legacy_flag_value() -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text("hello"))
}

#[mcp_tool(title = false)]
async fn non_string_title() -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text("hello"))
}

#[mcp_tool(name = "first" description = "missing comma")]
async fn missing_separator() -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text("hello"))
}

fn main() {}
