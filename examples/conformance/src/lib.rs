//! Conformance fixture server
//!
//! Implements the fixtures that the official MCP conformance suite
//! (<https://github.com/modelcontextprotocol/conformance>) expects from a
//! server under test, limited to what this SDK can express. The fixture
//! names and payloads mirror the TypeScript SDK's reference
//! `everythingServer.ts`.
//!
//! It also registers `echo`, `slow` and `fail`, which the cross-SDK wire
//! probes use to compare this SDK with `rmcp` and the TypeScript SDK.
//!
//! `tests/` replays wire sessions recorded from real clients (MCP Inspector
//! and Claude Code) against this server.

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use mcp_sdk::context::ToolContext;
use mcp_sdk::handler::prompts::PromptHandler;
use mcp_sdk::handler::resources::ResourceHandler;
use mcp_sdk::handler::tools::ToolHandler;
use mcp_sdk::protocol::messages::{
    PromptArgument, PromptInfo, ResourceInfo, ResourceTemplateInfo, ToolAnnotations, ToolInfo,
};
use mcp_sdk::server::McpServer;
use mcp_sdk::types::content::{CallToolResult, Content, PromptMessage, ResourceContent, Role};
use mcp_sdk::types::error::{PromptError, ResourceError, ToolError};
use serde_json::{json, Value};

const TEST_IMAGE_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==";

fn empty_schema() -> Value {
    json!({"type": "object", "properties": {}})
}

fn tool_info(name: &str, description: &str, input_schema: Value) -> ToolInfo {
    ToolInfo {
        name: name.to_string(),
        description: Some(description.to_string()),
        input_schema,
        annotations: None,
    }
}

fn embedded_text(uri: &str, mime_type: &str, text: &str) -> Content {
    Content::resource(ResourceContent {
        uri: uri.to_string(),
        mime_type: Some(mime_type.to_string()),
        text: Some(text.to_string()),
        blob: None,
    })
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// A tool whose result does not depend on its arguments.
struct FixedTool {
    info: ToolInfo,
    result: fn() -> Result<CallToolResult, ToolError>,
}

#[async_trait]
impl ToolHandler for FixedTool {
    fn info(&self) -> ToolInfo {
        self.info.clone()
    }

    async fn call(
        &self,
        _arguments: Value,
        _context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        (self.result)()
    }
}

struct EchoTool;

#[async_trait]
impl ToolHandler for EchoTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            annotations: Some(ToolAnnotations {
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                idempotent_hint: Some(true),
                open_world_hint: Some(false),
                ..Default::default()
            }),
            ..tool_info(
                "echo",
                "Echo the message back",
                json!({
                    "type": "object",
                    "properties": {"message": {"type": "string"}},
                    "required": ["message"]
                }),
            )
        }
    }

    async fn call(
        &self,
        arguments: Value,
        _context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        let message = arguments
            .get("message")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::InvalidParams("'message' must be a string".to_string()))?;
        Ok(CallToolResult::text(message))
    }
}

struct SlowTool;

#[async_trait]
impl ToolHandler for SlowTool {
    fn info(&self) -> ToolInfo {
        tool_info(
            "slow",
            "Sleep for `ms` milliseconds, then return \"done\"",
            json!({
                "type": "object",
                "properties": {"ms": {"type": "integer"}},
                "required": ["ms"]
            }),
        )
    }

    async fn call(
        &self,
        arguments: Value,
        context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        let ms = arguments
            .get("ms")
            .and_then(Value::as_u64)
            .ok_or_else(|| ToolError::InvalidParams("'ms' must be an integer".to_string()))?;
        tokio::select! {
            () = tokio::time::sleep(Duration::from_millis(ms)) => Ok(CallToolResult::text("done")),
            () = context.cancellation_token().cancelled() => {
                Err(ToolError::ExecutionError("cancelled".to_string()))
            }
        }
    }
}

fn tools() -> Vec<FixedTool> {
    vec![
        FixedTool {
            info: tool_info(
                "test_simple_text",
                "Tests simple text content response",
                empty_schema(),
            ),
            result: || {
                Ok(CallToolResult::text(
                    "This is a simple text response for testing.",
                ))
            },
        },
        FixedTool {
            info: tool_info(
                "test_image_content",
                "Tests image content response",
                empty_schema(),
            ),
            result: || {
                Ok(CallToolResult {
                    content: vec![Content::image(TEST_IMAGE_BASE64, "image/png")],
                    is_error: false,
                })
            },
        },
        FixedTool {
            info: tool_info(
                "test_embedded_resource",
                "Tests embedded resource content response",
                empty_schema(),
            ),
            result: || {
                Ok(CallToolResult {
                    content: vec![embedded_text(
                        "test://embedded-resource",
                        "text/plain",
                        "This is an embedded resource content.",
                    )],
                    is_error: false,
                })
            },
        },
        FixedTool {
            info: tool_info(
                "test_multiple_content_types",
                "Tests response with multiple content types (text, image, resource)",
                empty_schema(),
            ),
            result: || {
                Ok(CallToolResult {
                    content: vec![
                        Content::text("Multiple content types test:"),
                        Content::image(TEST_IMAGE_BASE64, "image/png"),
                        embedded_text(
                            "test://mixed-content-resource",
                            "application/json",
                            &json!({"test": "data", "value": 123}).to_string(),
                        ),
                    ],
                    is_error: false,
                })
            },
        },
        FixedTool {
            info: tool_info(
                "test_error_handling",
                "Tests error response handling",
                empty_schema(),
            ),
            result: || {
                Err(ToolError::ExecutionError(
                    "This tool intentionally returns an error for testing".to_string(),
                ))
            },
        },
        FixedTool {
            info: tool_info(
                "json_schema_2020_12_tool",
                "Tool with JSON Schema 2020-12 features for conformance testing (SEP-1613, SEP-2106)",
                json!({
                    "$schema": "https://json-schema.org/draft/2020-12/schema",
                    "type": "object",
                    "$defs": {
                        "address": {
                            "$anchor": "addressDef",
                            "type": "object",
                            "properties": {"street": {"type": "string"}, "city": {"type": "string"}}
                        }
                    },
                    "properties": {
                        "name": {"type": "string"},
                        "address": {"$ref": "#/$defs/address"},
                        "contactMethod": {"type": "string", "enum": ["phone", "email"]},
                        "phone": {"type": "string"},
                        "email": {"type": "string"}
                    },
                    "allOf": [{"anyOf": [{"required": ["phone"]}, {"required": ["email"]}]}],
                    "if": {"properties": {"contactMethod": {"const": "phone"}}, "required": ["contactMethod"]},
                    "then": {"required": ["phone"]},
                    "else": {"required": ["email"]},
                    "additionalProperties": false
                }),
            ),
            result: || Ok(CallToolResult::text("JSON Schema 2020-12 tool called")),
        },
        FixedTool {
            info: tool_info(
                "fail",
                "Always fails with a tool execution error",
                empty_schema(),
            ),
            result: || Err(ToolError::ExecutionError("boom".to_string())),
        },
    ]
}

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

struct StaticResource {
    info: ResourceInfo,
    content: ResourceContent,
}

#[async_trait]
impl ResourceHandler for StaticResource {
    fn info(&self) -> ResourceInfo {
        self.info.clone()
    }

    async fn read(&self, _uri: &str) -> Result<ResourceContent, ResourceError> {
        Ok(self.content.clone())
    }
}

struct TemplateResource;

const TEMPLATE: &str = "test://template/{id}/data";

#[async_trait]
impl ResourceHandler for TemplateResource {
    fn info(&self) -> ResourceInfo {
        ResourceInfo {
            uri: TEMPLATE.to_string(),
            name: "template".to_string(),
            description: Some("A resource template with parameter substitution".to_string()),
            mime_type: Some("application/json".to_string()),
        }
    }

    fn template_info(&self) -> Option<ResourceTemplateInfo> {
        Some(ResourceTemplateInfo {
            uri_template: TEMPLATE.to_string(),
            name: "template".to_string(),
            description: Some("A resource template with parameter substitution".to_string()),
            mime_type: Some("application/json".to_string()),
        })
    }

    fn is_template(&self) -> bool {
        true
    }

    async fn read(&self, uri: &str) -> Result<ResourceContent, ResourceError> {
        let id = uri
            .strip_prefix("test://template/")
            .and_then(|rest| rest.strip_suffix("/data"))
            .ok_or_else(|| ResourceError::InvalidUri(uri.to_string()))?;
        Ok(ResourceContent {
            uri: uri.to_string(),
            mime_type: Some("application/json".to_string()),
            text: Some(
                json!({"id": id, "templateTest": true, "data": format!("Data for ID: {id}")})
                    .to_string(),
            ),
            blob: None,
        })
    }
}

fn resources() -> Vec<StaticResource> {
    let static_resource = |uri: &str,
                           name: &str,
                           description: &str,
                           mime: &str,
                           text: Option<&str>,
                           blob: Option<&str>| {
        StaticResource {
            info: ResourceInfo {
                uri: uri.to_string(),
                name: name.to_string(),
                description: Some(description.to_string()),
                mime_type: Some(mime.to_string()),
            },
            content: ResourceContent {
                uri: uri.to_string(),
                mime_type: Some(mime.to_string()),
                text: text.map(str::to_string),
                blob: blob.map(str::to_string),
            },
        }
    };
    vec![
        static_resource(
            "test://static-text",
            "static-text",
            "A static text resource for testing",
            "text/plain",
            Some("This is the content of the static text resource."),
            None,
        ),
        static_resource(
            "test://static-binary",
            "static-binary",
            "A static binary resource (image) for testing",
            "image/png",
            None,
            Some(TEST_IMAGE_BASE64),
        ),
    ]
}

// ---------------------------------------------------------------------------
// Prompts
// ---------------------------------------------------------------------------

struct FixturePrompt {
    info: PromptInfo,
    render: fn(&HashMap<String, String>) -> Vec<PromptMessage>,
}

#[async_trait]
impl PromptHandler for FixturePrompt {
    fn info(&self) -> PromptInfo {
        self.info.clone()
    }

    async fn get(
        &self,
        arguments: HashMap<String, String>,
    ) -> Result<Vec<PromptMessage>, PromptError> {
        for arg in self.info.arguments.iter().flatten() {
            if arg.required == Some(true) && !arguments.contains_key(&arg.name) {
                return Err(PromptError::InvalidArguments(format!(
                    "missing argument '{}'",
                    arg.name
                )));
            }
        }
        Ok((self.render)(&arguments))
    }
}

fn user(content: Content) -> PromptMessage {
    PromptMessage {
        role: Role::User,
        content,
    }
}

fn required_arg(name: &str, description: &str) -> PromptArgument {
    PromptArgument {
        name: name.to_string(),
        description: Some(description.to_string()),
        required: Some(true),
    }
}

fn prompts() -> Vec<FixturePrompt> {
    let info = |name: &str, description: &str, arguments: Option<Vec<PromptArgument>>| PromptInfo {
        name: name.to_string(),
        description: Some(description.to_string()),
        arguments,
    };
    vec![
        FixturePrompt {
            info: info(
                "test_simple_prompt",
                "A simple prompt without arguments",
                None,
            ),
            render: |_| vec![user(Content::text("This is a simple prompt for testing."))],
        },
        FixturePrompt {
            info: info(
                "test_prompt_with_arguments",
                "A prompt with required arguments",
                Some(vec![
                    required_arg("arg1", "First test argument"),
                    required_arg("arg2", "Second test argument"),
                ]),
            ),
            render: |args| {
                vec![user(Content::text(format!(
                    "Prompt with arguments: arg1='{}', arg2='{}'",
                    args["arg1"], args["arg2"]
                )))]
            },
        },
        FixturePrompt {
            info: info(
                "test_prompt_with_embedded_resource",
                "A prompt that includes an embedded resource",
                Some(vec![required_arg(
                    "resourceUri",
                    "URI of the resource to embed",
                )]),
            ),
            render: |args| {
                vec![
                    user(embedded_text(
                        &args["resourceUri"],
                        "text/plain",
                        "Embedded resource content for testing.",
                    )),
                    user(Content::text("Please process the embedded resource above.")),
                ]
            },
        },
        FixturePrompt {
            info: info(
                "test_prompt_with_image",
                "A prompt that includes image content",
                None,
            ),
            render: |_| {
                vec![
                    user(Content::image(TEST_IMAGE_BASE64, "image/png")),
                    user(Content::text("Please analyze the image above.")),
                ]
            },
        },
    ]
}

/// Builds the fixture server.
#[must_use]
pub fn build_server() -> McpServer {
    let mut builder = McpServer::builder()
        .name("mcp-rust-sdk-conformance")
        .version(env!("CARGO_PKG_VERSION"))
        .tool(EchoTool)
        .tool(SlowTool)
        .resource(TemplateResource);
    for tool in tools() {
        builder = builder.tool(tool);
    }
    for resource in resources() {
        builder = builder.resource(resource);
    }
    for prompt in prompts() {
        builder = builder.prompt(prompt);
    }
    builder.build()
}
