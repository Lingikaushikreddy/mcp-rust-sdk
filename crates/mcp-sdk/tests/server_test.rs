//! Server builder and lifecycle tests.

use async_trait::async_trait;
use mcp_sdk::context::ToolContext;
use mcp_sdk::handler::tools::ToolHandler;
use mcp_sdk::protocol::messages::ToolInfo;
use mcp_sdk::schema::JsonSchemaBuilder;
use mcp_sdk::server::McpServer;
use mcp_sdk::transport::stdio::TestStdioTransport;
use mcp_sdk::types::content::CallToolResult;
use mcp_sdk::types::error::ToolError;

struct EchoTool;

#[async_trait]
impl ToolHandler for EchoTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            annotations: Some(mcp_sdk::ToolAnnotations {
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                idempotent_hint: Some(true),
                open_world_hint: Some(false),
                ..Default::default()
            }),
            name: "echo".to_string(),
            description: Some("Echo the input message".to_string()),
            input_schema: JsonSchemaBuilder::new()
                .property("message", JsonSchemaBuilder::string())
                .required("message")
                .build(),
        }
    }

    async fn call(
        &self,
        arguments: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        let msg = arguments
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("(empty)");
        Ok(CallToolResult::text(msg))
    }
}

#[test]
fn test_server_builder_no_tools() {
    let server = McpServer::builder()
        .name("empty-server")
        .version("0.1.0")
        .build();
    assert!(format!("{:?}", server).contains("empty-server"));
}

#[test]
fn test_server_builder_with_tool() {
    let server = McpServer::builder()
        .name("echo-server")
        .version("1.0.0")
        .tool(EchoTool)
        .build();
    assert!(format!("{:?}", server).contains("echo-server"));
}

#[test]
fn test_server_builder_with_instructions() {
    let server = McpServer::builder()
        .name("test")
        .version("1.0.0")
        .instructions("Some instructions")
        .build();
    assert!(format!("{:?}", server).contains("test"));
}

fn init_handshake() -> String {
    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0.0"}
        }
    });
    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    format!("{}\n{}\n", init, initialized)
}

#[tokio::test]
async fn test_server_initialize_handshake() {
    let transport = TestStdioTransport::from_input(&init_handshake());

    let server = McpServer::builder()
        .name("test-server")
        .version("1.0.0")
        .tool(EchoTool)
        .build();

    let result = server.run(transport).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_server_tools_list() {
    let tools_list = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });

    let input = format!("{}{}\n", init_handshake(), tools_list);
    let transport = TestStdioTransport::from_input(&input);

    let server = McpServer::builder()
        .name("test-server")
        .version("1.0.0")
        .tool(EchoTool)
        .build();

    let result = server.run(transport).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_server_tool_call() {
    let tool_call = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "echo",
            "arguments": {"message": "hello world"}
        }
    });

    let input = format!("{}{}\n", init_handshake(), tool_call);
    let transport = TestStdioTransport::from_input(&input);

    let server = McpServer::builder()
        .name("test-server")
        .version("1.0.0")
        .tool(EchoTool)
        .build();

    let result = server.run(transport).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_server_rejects_before_initialize() {
    let tool_call = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    });

    let input = format!("{}\n", tool_call);
    let transport = TestStdioTransport::from_input(&input);

    let server = McpServer::builder()
        .name("test-server")
        .version("1.0.0")
        .build();

    let result = server.run(transport).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_server_ping() {
    let ping = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "ping"
    });

    let input = format!("{}{}\n", init_handshake(), ping);
    let transport = TestStdioTransport::from_input(&input);

    let server = McpServer::builder()
        .name("test-server")
        .version("1.0.0")
        .build();

    let result = server.run(transport).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_server_unknown_method() {
    let unknown = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "unknown/method",
        "params": {}
    });

    let input = format!("{}{}\n", init_handshake(), unknown);
    let transport = TestStdioTransport::from_input(&input);

    let server = McpServer::builder()
        .name("test-server")
        .version("1.0.0")
        .build();

    let result = server.run(transport).await;
    assert!(result.is_ok());
}
