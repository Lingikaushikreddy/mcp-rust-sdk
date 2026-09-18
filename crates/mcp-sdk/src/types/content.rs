//! Content types for MCP tool results, resources, and prompts.
//!
//! These types represent the various kinds of content that can be returned
//! by MCP tools, read from resources, or included in prompt messages.

use serde::{Deserialize, Serialize};

/// A single piece of content returned by a tool or resource.
///
/// Content can be text, an image (base64-encoded), or an embedded resource.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum Content {
    /// Plain text content.
    #[serde(rename = "text")]
    Text {
        /// The text content.
        text: String,
    },
    /// Base64-encoded image content.
    #[serde(rename = "image")]
    Image {
        /// Base64-encoded image data.
        data: String,
        /// MIME type of the image (e.g., "image/png").
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    /// An embedded resource.
    #[serde(rename = "resource")]
    Resource {
        /// The embedded resource content.
        resource: ResourceContent,
    },
}

impl Content {
    /// Creates a text content item.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// Creates an image content item from base64 data.
    #[must_use]
    pub fn image(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self::Image {
            data: data.into(),
            mime_type: mime_type.into(),
        }
    }

    /// Creates a resource content item.
    #[must_use]
    pub fn resource(resource: ResourceContent) -> Self {
        Self::Resource { resource }
    }
}

/// The result of a tool call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CallToolResult {
    /// The content items produced by the tool.
    pub content: Vec<Content>,
    /// Whether the result represents an error from the tool's perspective.
    #[serde(
        rename = "isError",
        default,
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub is_error: bool,
}

impl CallToolResult {
    /// Creates a successful result with a single text content item.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![Content::text(text)],
            is_error: false,
        }
    }

    /// Creates an error result with a text description.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![Content::text(message)],
            is_error: true,
        }
    }

    /// Creates a result with multiple content items.
    #[must_use]
    pub fn with_content(content: Vec<Content>) -> Self {
        Self {
            content,
            is_error: false,
        }
    }
}

impl From<String> for CallToolResult {
    fn from(text: String) -> Self {
        Self::text(text)
    }
}

impl From<&str> for CallToolResult {
    fn from(text: &str) -> Self {
        Self::text(text)
    }
}

/// Content of a resource, which can be text or binary (blob).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceContent {
    /// The URI of the resource.
    pub uri: String,
    /// The MIME type of the resource content.
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Text content (mutually exclusive with `blob`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Base64-encoded binary content (mutually exclusive with `text`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

impl ResourceContent {
    /// Creates a text resource content.
    #[must_use]
    pub fn text(uri: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            mime_type: Some("text/plain".to_string()),
            text: Some(content.into()),
            blob: None,
        }
    }

    /// Creates a text resource content with a specific MIME type.
    #[must_use]
    pub fn text_with_mime(
        uri: impl Into<String>,
        content: impl Into<String>,
        mime_type: impl Into<String>,
    ) -> Self {
        Self {
            uri: uri.into(),
            mime_type: Some(mime_type.into()),
            text: Some(content.into()),
            blob: None,
        }
    }

    /// Creates a binary resource content from base64-encoded data.
    #[must_use]
    pub fn blob(
        uri: impl Into<String>,
        data: impl Into<String>,
        mime_type: impl Into<String>,
    ) -> Self {
        Self {
            uri: uri.into(),
            mime_type: Some(mime_type.into()),
            text: None,
            blob: Some(data.into()),
        }
    }
}

/// A message in a prompt template.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptMessage {
    /// The role of the message sender.
    pub role: Role,
    /// The content of the message.
    pub content: Content,
}

impl PromptMessage {
    /// Creates a user message with text content.
    #[must_use]
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: Content::text(text),
        }
    }

    /// Creates an assistant message with text content.
    #[must_use]
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: Content::text(text),
        }
    }
}

/// The role of a message sender in a prompt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// A user message.
    User,
    /// An assistant message.
    Assistant,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_text() {
        let content = Content::text("hello");
        let json = serde_json::to_value(&content).expect("serialize");
        assert_eq!(json["type"], "text");
        assert_eq!(json["text"], "hello");
    }

    #[test]
    fn test_content_image() {
        let content = Content::image("base64data", "image/png");
        let json = serde_json::to_value(&content).expect("serialize");
        assert_eq!(json["type"], "image");
        assert_eq!(json["data"], "base64data");
        assert_eq!(json["mimeType"], "image/png");
    }

    #[test]
    fn test_call_tool_result_text() {
        let result = CallToolResult::text("42");
        let json = serde_json::to_value(&result).expect("serialize");
        assert_eq!(json["content"][0]["type"], "text");
        assert_eq!(json["content"][0]["text"], "42");
        assert!(json.get("isError").is_none());
    }

    #[test]
    fn test_call_tool_result_error() {
        let result = CallToolResult::error("something went wrong");
        let json = serde_json::to_value(&result).expect("serialize");
        assert_eq!(json["isError"], true);
    }

    #[test]
    fn test_resource_content_text() {
        let resource = ResourceContent::text("file:///test.txt", "hello world");
        let json = serde_json::to_value(&resource).expect("serialize");
        assert_eq!(json["uri"], "file:///test.txt");
        assert_eq!(json["text"], "hello world");
        assert_eq!(json["mimeType"], "text/plain");
        assert!(json.get("blob").is_none());
    }

    #[test]
    fn test_prompt_message() {
        let msg = PromptMessage::user("Hello, please help me.");
        let json = serde_json::to_value(&msg).expect("serialize");
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"]["type"], "text");
    }

    #[test]
    fn test_content_roundtrip() {
        let content = Content::text("test");
        let json = serde_json::to_string(&content).expect("serialize");
        let back: Content = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(content, back);
    }
}
