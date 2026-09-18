//! MCP-specific message types built on top of JSON-RPC 2.0.
//!
//! This module defines all the request and response parameter types for
//! MCP methods such as `initialize`, `tools/list`, `tools/call`,
//! `resources/list`, `resources/read`, `prompts/list`, and `prompts/get`.

use serde::{Deserialize, Serialize};

use crate::protocol::capabilities::{ClientCapabilities, ServerCapabilities};
use crate::types::content::{Content, PromptMessage, ResourceContent};

// ---------------------------------------------------------------------------
// Implementation info
// ---------------------------------------------------------------------------

/// Information about an MCP implementation (client or server).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Implementation {
    /// The name of the implementation.
    pub name: String,
    /// The version of the implementation.
    pub version: String,
}

// ---------------------------------------------------------------------------
// Initialize
// ---------------------------------------------------------------------------

/// Parameters for the `initialize` request sent by the client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InitializeParams {
    /// The protocol version the client supports.
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    /// Capabilities that the client supports.
    pub capabilities: ClientCapabilities,
    /// Information about the client implementation.
    #[serde(rename = "clientInfo")]
    pub client_info: Implementation,
}

/// Result of the `initialize` request sent by the server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InitializeResult {
    /// The protocol version the server selected.
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    /// Capabilities that the server supports.
    pub capabilities: ServerCapabilities,
    /// Information about the server implementation.
    #[serde(rename = "serverInfo")]
    pub server_info: Implementation,
    /// Optional instructions for using this server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

// ---------------------------------------------------------------------------
// Ping
// ---------------------------------------------------------------------------

/// Result of a `ping` request. An empty object.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PingResult {}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// Metadata about a registered tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolInfo {
    /// The unique name of the tool.
    pub name: String,
    /// A human-readable description of the tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The JSON Schema describing the tool's input parameters.
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// Parameters for the `tools/list` request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ListToolsParams {
    /// Optional cursor for pagination.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Result of the `tools/list` request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ListToolsResult {
    /// The list of available tools.
    pub tools: Vec<ToolInfo>,
    /// Optional cursor for the next page.
    #[serde(rename = "nextCursor", skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Parameters for the `tools/call` request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CallToolParams {
    /// The name of the tool to call.
    pub name: String,
    /// The arguments to pass to the tool.
    #[serde(default)]
    pub arguments: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

/// Metadata about a registered resource.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceInfo {
    /// The URI of the resource.
    pub uri: String,
    /// A human-readable name for the resource.
    pub name: String,
    /// A description of the resource.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The MIME type of the resource.
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// Metadata about a resource template.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceTemplateInfo {
    /// The URI template (RFC 6570) for the resource.
    #[serde(rename = "uriTemplate")]
    pub uri_template: String,
    /// A human-readable name for the resource template.
    pub name: String,
    /// A description of the resource template.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The MIME type of resources matching this template.
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// Parameters for the `resources/list` request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ListResourcesParams {
    /// Optional cursor for pagination.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Result of the `resources/list` request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ListResourcesResult {
    /// The list of available resources.
    pub resources: Vec<ResourceInfo>,
    /// Optional cursor for the next page.
    #[serde(rename = "nextCursor", skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Result of the `resources/templates/list` request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ListResourceTemplatesResult {
    /// The list of available resource templates.
    #[serde(rename = "resourceTemplates")]
    pub resource_templates: Vec<ResourceTemplateInfo>,
    /// Optional cursor for the next page.
    #[serde(rename = "nextCursor", skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Parameters for the `resources/read` request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReadResourceParams {
    /// The URI of the resource to read.
    pub uri: String,
}

/// Result of the `resources/read` request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReadResourceResult {
    /// The contents of the resource.
    pub contents: Vec<ResourceContent>,
}

// ---------------------------------------------------------------------------
// Prompts
// ---------------------------------------------------------------------------

/// Metadata about a registered prompt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptInfo {
    /// The unique name of the prompt.
    pub name: String,
    /// A description of the prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The arguments accepted by the prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<PromptArgument>>,
}

/// A single argument for a prompt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptArgument {
    /// The name of the argument.
    pub name: String,
    /// A description of the argument.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether the argument is required.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

/// Parameters for the `prompts/list` request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ListPromptsParams {
    /// Optional cursor for pagination.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Result of the `prompts/list` request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ListPromptsResult {
    /// The list of available prompts.
    pub prompts: Vec<PromptInfo>,
    /// Optional cursor for the next page.
    #[serde(rename = "nextCursor", skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Parameters for the `prompts/get` request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GetPromptParams {
    /// The name of the prompt to get.
    pub name: String,
    /// The arguments to fill the prompt with.
    #[serde(default)]
    pub arguments: std::collections::HashMap<String, String>,
}

/// Result of the `prompts/get` request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GetPromptResult {
    /// An optional description of the prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The messages generated by the prompt.
    pub messages: Vec<PromptMessage>,
}

// ---------------------------------------------------------------------------
// Logging
// ---------------------------------------------------------------------------

/// Parameters for the `logging/setLevel` request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SetLevelParams {
    /// The logging level to set.
    pub level: LoggingLevel,
}

/// MCP logging levels.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum LoggingLevel {
    /// Debug level.
    Debug,
    /// Info level.
    Info,
    /// Notice level.
    Notice,
    /// Warning level.
    Warning,
    /// Error level.
    Error,
    /// Critical level.
    Critical,
    /// Alert level.
    Alert,
    /// Emergency level.
    Emergency,
}

/// Parameters for the `notifications/message` log notification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LogMessageParams {
    /// The severity level.
    pub level: LoggingLevel,
    /// The logger name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logger: Option<String>,
    /// The log data.
    pub data: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Progress
// ---------------------------------------------------------------------------

/// Parameters for the `notifications/progress` notification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProgressParams {
    /// The progress token identifying the operation.
    #[serde(rename = "progressToken")]
    pub progress_token: serde_json::Value,
    /// Current progress value.
    pub progress: f64,
    /// Total expected value (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,
}

// ---------------------------------------------------------------------------
// Method constants
// ---------------------------------------------------------------------------

/// Well-known MCP method names.
pub mod methods {
    /// The `initialize` method.
    pub const INITIALIZE: &str = "initialize";
    /// The `notifications/initialized` notification.
    pub const INITIALIZED: &str = "notifications/initialized";
    /// The `ping` method.
    pub const PING: &str = "ping";
    /// The `tools/list` method.
    pub const TOOLS_LIST: &str = "tools/list";
    /// The `tools/call` method.
    pub const TOOLS_CALL: &str = "tools/call";
    /// The `resources/list` method.
    pub const RESOURCES_LIST: &str = "resources/list";
    /// The `resources/read` method.
    pub const RESOURCES_READ: &str = "resources/read";
    /// The `resources/templates/list` method.
    pub const RESOURCES_TEMPLATES_LIST: &str = "resources/templates/list";
    /// The `prompts/list` method.
    pub const PROMPTS_LIST: &str = "prompts/list";
    /// The `prompts/get` method.
    pub const PROMPTS_GET: &str = "prompts/get";
    /// The `logging/setLevel` method.
    pub const LOGGING_SET_LEVEL: &str = "logging/setLevel";
    /// The `notifications/message` notification.
    pub const NOTIFICATION_MESSAGE: &str = "notifications/message";
    /// The `notifications/progress` notification.
    pub const NOTIFICATION_PROGRESS: &str = "notifications/progress";
    /// The `notifications/cancelled` notification.
    pub const NOTIFICATION_CANCELLED: &str = "notifications/cancelled";
    /// The `notifications/tools/list_changed` notification.
    pub const NOTIFICATION_TOOLS_LIST_CHANGED: &str = "notifications/tools/list_changed";
    /// The `notifications/resources/list_changed` notification.
    pub const NOTIFICATION_RESOURCES_LIST_CHANGED: &str = "notifications/resources/list_changed";
    /// The `notifications/prompts/list_changed` notification.
    pub const NOTIFICATION_PROMPTS_LIST_CHANGED: &str = "notifications/prompts/list_changed";
}

/// Helper to build a `Content::Text` from a string.
impl From<String> for Content {
    fn from(text: String) -> Self {
        Content::Text { text }
    }
}

impl From<&str> for Content {
    fn from(text: &str) -> Self {
        Content::Text {
            text: text.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize_params_serialization() {
        let params = InitializeParams {
            protocol_version: "2025-11-25".to_string(),
            capabilities: ClientCapabilities::default(),
            client_info: Implementation {
                name: "test-client".to_string(),
                version: "1.0.0".to_string(),
            },
        };
        let json = serde_json::to_value(&params).expect("serialize");
        assert_eq!(json["protocolVersion"], "2025-11-25");
        assert_eq!(json["clientInfo"]["name"], "test-client");
    }

    #[test]
    fn test_tool_info_serialization() {
        let tool = ToolInfo {
            name: "calculate".to_string(),
            description: Some("Do math".to_string()),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "expression": {"type": "string"}
                },
                "required": ["expression"]
            }),
        };
        let json = serde_json::to_value(&tool).expect("serialize");
        assert_eq!(json["name"], "calculate");
        assert_eq!(json["inputSchema"]["type"], "object");
    }

    #[test]
    fn test_call_tool_params_deserialization() {
        let json = r#"{"name":"add","arguments":{"a":1,"b":2}}"#;
        let params: CallToolParams = serde_json::from_str(json).expect("deserialize");
        assert_eq!(params.name, "add");
        assert_eq!(params.arguments["a"], 1);
    }

    #[test]
    fn test_prompt_info_serialization() {
        let prompt = PromptInfo {
            name: "review".to_string(),
            description: Some("Code review prompt".to_string()),
            arguments: Some(vec![
                PromptArgument {
                    name: "code".to_string(),
                    description: Some("The code to review".to_string()),
                    required: Some(true),
                },
                PromptArgument {
                    name: "language".to_string(),
                    description: Some("Programming language".to_string()),
                    required: Some(false),
                },
            ]),
        };
        let json = serde_json::to_value(&prompt).expect("serialize");
        assert_eq!(json["arguments"][0]["name"], "code");
        assert_eq!(json["arguments"][0]["required"], true);
    }

    #[test]
    fn test_logging_level_ordering() {
        assert!(LoggingLevel::Debug < LoggingLevel::Info);
        assert!(LoggingLevel::Info < LoggingLevel::Warning);
        assert!(LoggingLevel::Warning < LoggingLevel::Error);
        assert!(LoggingLevel::Error < LoggingLevel::Emergency);
    }

    #[test]
    fn test_resource_info_serialization() {
        let resource = ResourceInfo {
            uri: "file:///test.txt".to_string(),
            name: "test.txt".to_string(),
            description: Some("A test file".to_string()),
            mime_type: Some("text/plain".to_string()),
        };
        let json = serde_json::to_value(&resource).expect("serialize");
        assert_eq!(json["uri"], "file:///test.txt");
        assert_eq!(json["mimeType"], "text/plain");
    }
}
