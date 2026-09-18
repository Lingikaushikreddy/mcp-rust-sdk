//! Tests for the #[mcp_tool] proc macro.

use mcp_sdk::context::ToolContext;
use mcp_sdk::handler::tools::ToolHandler;
use mcp_sdk::types::content::CallToolResult;
use mcp_sdk::types::error::ToolError;

use mcp_macros::mcp_tool;

/// Add two numbers together
#[mcp_tool(description = "Add two numbers")]
async fn add(a: f64, b: f64) -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text((a + b).to_string()))
}

#[mcp_tool(name = "custom_echo", description = "Echo a message")]
async fn echo_message(message: String) -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text(message))
}

#[mcp_tool]
async fn no_params() -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text("hello"))
}

#[test]
fn test_tool_info_add() {
    let handler = add();
    let info = handler.info();
    assert_eq!(info.name, "add");
    assert_eq!(info.description.as_deref(), Some("Add two numbers"));
    assert!(info.input_schema.get("properties").is_some());
}

#[test]
fn test_tool_info_custom_name() {
    let handler = echo_message();
    let info = handler.info();
    assert_eq!(info.name, "custom_echo");
    assert_eq!(info.description.as_deref(), Some("Echo a message"));
}

#[test]
fn test_tool_info_no_params() {
    let handler = no_params();
    let info = handler.info();
    assert_eq!(info.name, "no_params");
    assert_eq!(info.input_schema["type"], "object");
}

#[tokio::test]
async fn test_tool_call_add() {
    let handler = add();
    let ctx = ToolContext::default();
    let result = handler
        .call(serde_json::json!({"a": 3.0, "b": 4.0}), &ctx)
        .await
        .expect("call should succeed");
    assert_eq!(
        result.content[0],
        mcp_sdk::types::content::Content::text("7")
    );
}

#[tokio::test]
async fn test_tool_call_echo() {
    let handler = echo_message();
    let ctx = ToolContext::default();
    let result = handler
        .call(serde_json::json!({"message": "hello world"}), &ctx)
        .await
        .expect("call should succeed");
    assert_eq!(
        result.content[0],
        mcp_sdk::types::content::Content::text("hello world")
    );
}

#[tokio::test]
async fn test_tool_call_invalid_params() {
    let handler = add();
    let ctx = ToolContext::default();
    let result = handler
        .call(serde_json::json!({"wrong": "params"}), &ctx)
        .await;
    assert!(result.is_err());
}
