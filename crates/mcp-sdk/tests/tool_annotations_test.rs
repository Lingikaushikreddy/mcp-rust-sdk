//! Tool annotations survive the SDK's wire and registration boundaries.

use async_trait::async_trait;
use mcp_sdk::prelude::*;
use mcp_sdk::transport::stdio::TestStdioTransport;
use serde_json::{json, Value};

fn annotated_echo() -> Value {
    json!({
        "name": "echo",
        "description": "Echo a message",
        "inputSchema": {"type": "object"},
        "annotations": {
            "title": "Echo",
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": false
        }
    })
}

#[test]
fn annotations_preserve_explicit_boolean_values_and_wire_names() {
    let wire = annotated_echo();
    let tool: ToolInfo = serde_json::from_value(wire.clone()).expect("valid annotated tool");
    assert_eq!(serde_json::to_value(tool).expect("serialize tool"), wire);
}

#[test]
fn legacy_metadata_does_not_acquire_invented_annotations() {
    let wire = json!({"name": "legacy", "inputSchema": {"type": "object"}});
    let tool: ToolInfo = serde_json::from_value(wire.clone()).expect("legacy tool");
    assert_eq!(serde_json::to_value(tool).expect("serialize tool"), wire);
}

#[test]
fn partial_annotations_preserve_unspecified_hints() {
    let wire = json!({
        "name": "partial", "inputSchema": {"type": "object"},
        "annotations": {"destructiveHint": false}
    });
    let tool: ToolInfo = serde_json::from_value(wire.clone()).expect("partial annotations");
    assert_eq!(serde_json::to_value(tool).expect("serialize tool"), wire);
}

#[test]
fn empty_annotations_do_not_assert_safe_behavior() {
    let wire = json!({"name": "unknown", "inputSchema": {}, "annotations": {}});
    let tool: ToolInfo = serde_json::from_value(wire.clone()).expect("empty annotations");
    assert_eq!(serde_json::to_value(tool).expect("serialize tool"), wire);
}

#[test]
fn present_hints_require_boolean_values() {
    for hint in [
        "readOnlyHint",
        "destructiveHint",
        "idempotentHint",
        "openWorldHint",
    ] {
        for invalid in [json!("true"), json!(0), json!([]), json!({}), Value::Null] {
            let mut wire = annotated_echo();
            wire["annotations"][hint] = invalid.clone();
            assert!(
                serde_json::from_value::<ToolInfo>(wire).is_err(),
                "{hint} must reject {invalid}"
            );
        }
    }
}

struct AnnotatedEcho;

#[async_trait]
impl ToolHandler for AnnotatedEcho {
    fn info(&self) -> ToolInfo {
        serde_json::from_value(annotated_echo()).expect("valid tool metadata")
    }

    async fn call(
        &self,
        arguments: Value,
        _context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        Ok(CallToolResult::text(
            arguments["message"].as_str().unwrap_or_default(),
        ))
    }
}

#[tokio::test]
async fn tools_list_emits_annotations_after_registration_and_handshake() {
    let messages = [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25", "capabilities": {},
            "clientInfo": {"name": "annotation-client", "version": "1.0"}
        }}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    ];
    let input = messages
        .iter()
        .map(|message| format!("{message}\n"))
        .collect::<String>();
    let transport = TestStdioTransport::from_input(&input);
    let output = transport.output_handle();
    McpServer::builder()
        .tool(AnnotatedEcho)
        .build()
        .run(transport)
        .await
        .expect("server runs");
    let bytes = output.lock().await;
    let wire = std::str::from_utf8(&bytes).expect("UTF-8 response");
    let responses = wire
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON response"));
    let tools = responses
        .into_iter()
        .find(|response| response["id"] == 2)
        .expect("tools/list response");
    assert_eq!(
        tools["result"]["tools"],
        json!([{
            "name": "echo", "description": "Echo a message", "inputSchema": {"type": "object"},
            "annotations": {"title": "Echo", "readOnlyHint": true, "destructiveHint": false,
                "idempotentHint": true, "openWorldHint": false}
        }])
    );
}
