//! MCP capability negotiation types.
//!
//! Capabilities are exchanged during the `initialize` handshake to declare
//! which MCP features the server and client support. The server advertises
//! its capabilities (tools, resources, prompts, logging) and the client
//! declares what it can handle.

use serde::{Deserialize, Serialize};

/// The current MCP protocol version supported by this SDK.
pub const PROTOCOL_VERSION: &str = "2025-11-25";

/// Capabilities declared by the MCP server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ServerCapabilities {
    /// Tool-related capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
    /// Resource-related capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesCapability>,
    /// Prompt-related capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PromptsCapability>,
    /// Logging capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<LoggingCapability>,
}

/// Tool-related server capabilities.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ToolsCapability {
    /// Whether the tool list may change at runtime.
    #[serde(rename = "listChanged", skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Resource-related server capabilities.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ResourcesCapability {
    /// Whether the server supports resource subscriptions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribe: Option<bool>,
    /// Whether the resource list may change at runtime.
    #[serde(rename = "listChanged", skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Prompt-related server capabilities.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PromptsCapability {
    /// Whether the prompt list may change at runtime.
    #[serde(rename = "listChanged", skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Logging capabilities.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LoggingCapability {}

/// Capabilities declared by the MCP client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ClientCapabilities {
    /// Client root capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<RootsCapability>,
    /// Client sampling capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<SamplingCapability>,
}

/// Client capability for listing root directories.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RootsCapability {
    /// Whether the roots list may change at runtime.
    #[serde(rename = "listChanged", skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Client capability for sampling (LLM completion).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SamplingCapability {}

/// Builder for constructing [`ServerCapabilities`].
///
/// # Example
///
/// ```rust
/// use mcp_sdk::protocol::capabilities::ServerCapabilitiesBuilder;
///
/// let caps = ServerCapabilitiesBuilder::new()
///     .enable_tools(false)
///     .enable_resources(false)
///     .enable_prompts(false)
///     .enable_logging()
///     .build();
/// ```
#[derive(Debug, Default)]
pub struct ServerCapabilitiesBuilder {
    tools: Option<ToolsCapability>,
    resources: Option<ResourcesCapability>,
    prompts: Option<PromptsCapability>,
    logging: Option<LoggingCapability>,
}

impl ServerCapabilitiesBuilder {
    /// Creates a new capabilities builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enables tool capabilities.
    ///
    /// `list_changed` indicates whether the server will notify clients of
    /// changes to the tool list.
    #[must_use]
    pub fn enable_tools(mut self, list_changed: bool) -> Self {
        self.tools = Some(ToolsCapability {
            list_changed: if list_changed { Some(true) } else { None },
        });
        self
    }

    /// Enables resource capabilities.
    ///
    /// `list_changed` indicates whether the server will notify clients of
    /// changes to the resource list.
    #[must_use]
    pub fn enable_resources(mut self, list_changed: bool) -> Self {
        self.resources = Some(ResourcesCapability {
            subscribe: None,
            list_changed: if list_changed { Some(true) } else { None },
        });
        self
    }

    /// Enables prompt capabilities.
    ///
    /// `list_changed` indicates whether the server will notify clients of
    /// changes to the prompt list.
    #[must_use]
    pub fn enable_prompts(mut self, list_changed: bool) -> Self {
        self.prompts = Some(PromptsCapability {
            list_changed: if list_changed { Some(true) } else { None },
        });
        self
    }

    /// Enables logging capabilities.
    #[must_use]
    pub fn enable_logging(mut self) -> Self {
        self.logging = Some(LoggingCapability {});
        self
    }

    /// Builds the [`ServerCapabilities`].
    #[must_use]
    pub fn build(self) -> ServerCapabilities {
        ServerCapabilities {
            tools: self.tools,
            resources: self.resources,
            prompts: self.prompts,
            logging: self.logging,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_capabilities_builder() {
        let caps = ServerCapabilitiesBuilder::new()
            .enable_tools(false)
            .enable_resources(true)
            .enable_logging()
            .build();

        assert!(caps.tools.is_some());
        assert!(caps.tools.as_ref().and_then(|t| t.list_changed).is_none());
        assert!(caps.resources.is_some());
        assert_eq!(
            caps.resources.as_ref().and_then(|r| r.list_changed),
            Some(true)
        );
        assert!(caps.prompts.is_none());
        assert!(caps.logging.is_some());
    }

    #[test]
    fn test_capabilities_serialization() {
        let caps = ServerCapabilities {
            tools: Some(ToolsCapability { list_changed: None }),
            resources: None,
            prompts: None,
            logging: None,
        };
        let json = serde_json::to_value(&caps).expect("serialize");
        assert!(json.get("tools").is_some());
        assert!(json.get("resources").is_none());
    }

    #[test]
    fn test_capabilities_deserialization() {
        let json = r#"{"tools":{},"logging":{}}"#;
        let caps: ServerCapabilities = serde_json::from_str(json).expect("deserialize");
        assert!(caps.tools.is_some());
        assert!(caps.logging.is_some());
        assert!(caps.resources.is_none());
    }

    #[test]
    fn test_empty_capabilities() {
        let caps = ServerCapabilities::default();
        let json = serde_json::to_value(&caps).expect("serialize");
        let obj = json.as_object().expect("object");
        assert!(obj.is_empty());
    }
}
