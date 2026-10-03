//! Protocol type serialization and deserialization tests.

use mcp_sdk::protocol::capabilities::{
    ServerCapabilities, ServerCapabilitiesBuilder, PROTOCOL_VERSION,
};
use mcp_sdk::protocol::messages::*;
use mcp_sdk::protocol::types::*;
use mcp_sdk::types::content::*;
use mcp_sdk::types::error::*;

#[test]
fn test_parse_initialize_request() {
    let json = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            }
        }
    }"#;

    let msg = parse_jsonrpc_message(json).expect("should parse");
    match msg {
        JsonRpcMessage::Request(req) => {
            assert_eq!(req.method, "initialize");
            let params: InitializeParams =
                serde_json::from_value(req.params.expect("params")).expect("deserialize");
            assert_eq!(params.protocol_version, "2025-11-25");
            assert_eq!(params.client_info.name, "test-client");
        }
        _ => panic!("Expected request"),
    }
}

#[test]
fn test_initialize_result_serialization() {
    let result = InitializeResult {
        protocol_version: PROTOCOL_VERSION.to_string(),
        capabilities: ServerCapabilitiesBuilder::new().enable_tools(false).build(),
        server_info: Implementation {
            name: "test-server".to_string(),
            version: "0.1.0".to_string(),
        },
        instructions: Some("Test instructions".to_string()),
    };

    let json = serde_json::to_value(&result).expect("serialize");
    assert_eq!(json["protocolVersion"], PROTOCOL_VERSION);
    assert!(json["capabilities"]["tools"].is_object());
    assert_eq!(json["serverInfo"]["name"], "test-server");
    assert_eq!(json["instructions"], "Test instructions");
}

#[test]
fn test_tools_list_result() {
    let result = ListToolsResult {
        tools: vec![ToolInfo {
            annotations: None,
            name: "add".to_string(),
            description: Some("Add numbers".to_string()),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "a": {"type": "number"},
                    "b": {"type": "number"}
                },
                "required": ["a", "b"]
            }),
        }],
        next_cursor: None,
    };

    let json = serde_json::to_value(&result).expect("serialize");
    assert_eq!(json["tools"].as_array().expect("array").len(), 1);
    assert_eq!(json["tools"][0]["name"], "add");
    assert!(json.get("nextCursor").is_none());
}

#[test]
fn test_call_tool_params() {
    let json = r#"{"name":"add","arguments":{"a":1,"b":2}}"#;
    let params: CallToolParams = serde_json::from_str(json).expect("deserialize");
    assert_eq!(params.name, "add");
    assert_eq!(params.arguments["a"], 1);
    assert_eq!(params.arguments["b"], 2);
}

#[test]
fn test_call_tool_result_serialization() {
    let result = CallToolResult::text("42");
    let json = serde_json::to_value(&result).expect("serialize");
    assert_eq!(json["content"][0]["type"], "text");
    assert_eq!(json["content"][0]["text"], "42");
    assert!(json.get("isError").is_none()); // false should be omitted
}

#[test]
fn test_call_tool_result_error_serialization() {
    let result = CallToolResult::error("Division by zero");
    let json = serde_json::to_value(&result).expect("serialize");
    assert_eq!(json["isError"], true);
    assert_eq!(json["content"][0]["text"], "Division by zero");
}

#[test]
fn test_jsonrpc_error_codes() {
    let err = JsonRpcError::parse_error(None);
    assert_eq!(err.code, -32700);

    let err = JsonRpcError::invalid_request(None);
    assert_eq!(err.code, -32600);

    let err = JsonRpcError::method_not_found(None);
    assert_eq!(err.code, -32601);

    let err = JsonRpcError::invalid_params(None);
    assert_eq!(err.code, -32602);

    let err = JsonRpcError::internal_error(None);
    assert_eq!(err.code, -32603);
}

#[test]
fn test_notification_format() {
    let msg = JsonRpcMessage::notification("notifications/initialized", None);
    let json = serde_json::to_value(&msg).expect("serialize");
    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["method"], "notifications/initialized");
    assert!(json.get("id").is_none());
}

#[test]
fn test_error_response_format() {
    let msg = JsonRpcMessage::error_response(
        RequestId::Number(1),
        JsonRpcError::method_not_found(Some("unknown/method".to_string())),
    );
    let json = serde_json::to_value(&msg).expect("serialize");
    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["id"], 1);
    assert_eq!(json["error"]["code"], -32601);
}

#[test]
fn test_resource_content_roundtrip() {
    let content = ResourceContent::text("file:///test.txt", "hello");
    let json = serde_json::to_string(&content).expect("serialize");
    let back: ResourceContent = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(content, back);
}

#[test]
fn test_prompt_message_roundtrip() {
    let msg = PromptMessage::user("Hello");
    let json = serde_json::to_string(&msg).expect("serialize");
    let back: PromptMessage = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(msg, back);
}

#[test]
fn test_prompt_info_with_arguments() {
    let prompt = PromptInfo {
        name: "review".to_string(),
        description: Some("Code review".to_string()),
        arguments: Some(vec![PromptArgument {
            name: "code".to_string(),
            description: Some("Code to review".to_string()),
            required: Some(true),
        }]),
    };

    let json = serde_json::to_value(&prompt).expect("serialize");
    let back: PromptInfo = serde_json::from_value(json).expect("deserialize");
    assert_eq!(prompt, back);
}

#[test]
fn test_capabilities_empty_when_nothing_registered() {
    let caps = ServerCapabilities::default();
    let json = serde_json::to_value(&caps).expect("serialize");
    // Empty object -- no capabilities declared
    assert!(json.as_object().expect("object").is_empty());
}

#[test]
fn test_capabilities_with_tools() {
    let caps = ServerCapabilitiesBuilder::new().enable_tools(true).build();
    let json = serde_json::to_value(&caps).expect("serialize");
    assert!(json["tools"]["listChanged"].as_bool().expect("bool"));
}
