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

#[mcp_tool(destructive)]
async fn delete_item() -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text("deleted"))
}

#[mcp_tool(
    title = "Read, \"inspect\" and summarize",
    read_only_hint = true,
    destructive_hint = false,
    idempotent_hint = true,
    open_world_hint = false
)]
async fn inspect_item() -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text("inspected"))
}

#[mcp_tool(
    read_only_hint = false,
    destructive_hint = true,
    idempotent_hint = false,
    open_world_hint = true
)]
async fn update_item() -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text("updated"))
}

#[mcp_tool(read_only_hint = false)]
async fn partially_annotated() -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text("partial"))
}

#[mcp_tool(title = "Display title")]
async fn titled_item() -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text("titled"))
}

#[test]
fn explicit_annotation_booleans_are_preserved_in_tool_info() {
    let wire_info = serde_json::to_value(inspect_item().info()).unwrap();
    assert_eq!(
        wire_info["annotations"],
        serde_json::json!({
            "title": "Read, \"inspect\" and summarize",
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": false,
        })
    );

    let wire_info = serde_json::to_value(update_item().info()).unwrap();
    assert_eq!(
        wire_info["annotations"],
        serde_json::json!({
            "readOnlyHint": false,
            "destructiveHint": true,
            "idempotentHint": false,
            "openWorldHint": true,
        })
    );
}

#[test]
fn unspecified_annotation_hints_are_omitted() {
    let wire_info = serde_json::to_value(partially_annotated().info()).unwrap();
    assert_eq!(
        wire_info["annotations"],
        serde_json::json!({ "readOnlyHint": false })
    );
}

#[test]
fn annotation_title_can_be_supplied_without_hints() {
    let wire_info = serde_json::to_value(titled_item().info()).unwrap();
    assert_eq!(
        wire_info["annotations"],
        serde_json::json!({ "title": "Display title" })
    );
}

#[test]
fn legacy_destructive_flag_is_advertised_in_tool_info() {
    let wire_info = serde_json::to_value(delete_item().info()).unwrap();
    assert_eq!(
        wire_info["annotations"],
        serde_json::json!({ "destructiveHint": true })
    );
}

#[test]
fn tool_without_annotation_options_omits_annotations() {
    let wire_info = serde_json::to_value(no_params().info()).unwrap();
    assert!(wire_info.get("annotations").is_none());
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
