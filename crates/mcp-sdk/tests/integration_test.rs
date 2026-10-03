//! Full server integration tests.

use async_trait::async_trait;
use mcp_sdk::context::ToolContext;
use mcp_sdk::handler::tools::ToolHandler;
use mcp_sdk::protocol::messages::ToolInfo;
use mcp_sdk::schema::JsonSchemaBuilder;
use mcp_sdk::server::McpServer;
use mcp_sdk::transport::stdio::TestStdioTransport;
use mcp_sdk::types::content::CallToolResult;
use mcp_sdk::types::error::ToolError;

struct AddTool;

#[async_trait]
impl ToolHandler for AddTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            annotations: Some(mcp_sdk::ToolAnnotations {
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                idempotent_hint: Some(true),
                open_world_hint: Some(false),
                ..Default::default()
            }),
            name: "add".to_string(),
            description: Some("Add two numbers".to_string()),
            input_schema: JsonSchemaBuilder::new()
                .property("a", JsonSchemaBuilder::number())
                .property("b", JsonSchemaBuilder::number())
                .required("a")
                .required("b")
                .build(),
        }
    }

    async fn call(
        &self,
        arguments: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        let a = arguments
            .get("a")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| ToolError::InvalidParams("Missing 'a'".to_string()))?;
        let b = arguments
            .get("b")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| ToolError::InvalidParams("Missing 'b'".to_string()))?;
        Ok(CallToolResult::text((a + b).to_string()))
    }
}

struct FailTool;

#[async_trait]
impl ToolHandler for FailTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            annotations: Some(mcp_sdk::ToolAnnotations {
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                idempotent_hint: Some(true),
                open_world_hint: Some(false),
                ..Default::default()
            }),
            name: "fail".to_string(),
            description: Some("Always fails".to_string()),
            input_schema: JsonSchemaBuilder::new().build(),
        }
    }

    async fn call(
        &self,
        _arguments: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        Err(ToolError::ExecutionError("Intentional failure".to_string()))
    }
}

fn init_handshake() -> String {
    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}
        }
    });
    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    format!("{}\n{}\n", init, initialized)
}

async fn run_server_with_input(input: &str) -> Vec<u8> {
    let transport = TestStdioTransport::from_input(input);
    let output_handle = transport.output_handle();

    let server = McpServer::builder()
        .name("test-server")
        .version("1.0.0")
        .tool(AddTool)
        .tool(FailTool)
        .build();

    server
        .run(transport)
        .await
        .expect("server should not crash");

    let output = output_handle.lock().await;
    output.clone()
}

#[tokio::test]
async fn test_full_lifecycle() {
    let tools_list = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list"
    });
    let tool_call = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {"name": "add", "arguments": {"a": 10, "b": 20}}
    });

    let input = format!("{}{}\n{}\n", init_handshake(), tools_list, tool_call);
    let output = run_server_with_input(&input).await;
    let output_str = String::from_utf8(output).expect("utf8");

    // Should contain the initialize response
    assert!(output_str.contains("protocolVersion"));
    // Should contain the tool call result
    assert!(output_str.contains("30"));
}

#[tokio::test]
async fn test_tool_error_handling() {
    let tool_call = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {"name": "fail", "arguments": {}}
    });

    let input = format!("{}{}\n", init_handshake(), tool_call);
    let output = run_server_with_input(&input).await;
    let output_str = String::from_utf8(output).expect("utf8");

    // Should contain an error response
    assert!(output_str.contains("error"));
    assert!(output_str.contains("Intentional failure"));
}

#[tokio::test]
async fn test_unknown_tool_call() {
    let tool_call = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {"name": "nonexistent", "arguments": {}}
    });

    let input = format!("{}{}\n", init_handshake(), tool_call);
    let output = run_server_with_input(&input).await;
    let output_str = String::from_utf8(output).expect("utf8");

    assert!(output_str.contains("Unknown tool"));
}

#[tokio::test]
async fn test_multiple_sequential_tool_calls() {
    let call1 = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {"name": "add", "arguments": {"a": 1, "b": 2}}
    });
    let call2 = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {"name": "add", "arguments": {"a": 10, "b": 20}}
    });

    let input = format!("{}{}\n{}\n", init_handshake(), call1, call2);
    let output = run_server_with_input(&input).await;
    let output_str = String::from_utf8(output).expect("utf8");

    assert!(output_str.contains("3"));
    assert!(output_str.contains("30"));
}

#[tokio::test]
async fn test_graceful_eof() {
    let input = init_handshake();
    let output = run_server_with_input(&input).await;
    assert!(!output.is_empty()); // Should at least have initialize response
}
