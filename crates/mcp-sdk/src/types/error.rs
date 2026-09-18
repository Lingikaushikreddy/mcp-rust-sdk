//! Error types for the MCP SDK.
//!
//! This module provides strongly-typed error types that map to JSON-RPC 2.0
//! error codes used by the Model Context Protocol.

use serde::{Deserialize, Serialize};

/// Standard JSON-RPC 2.0 error codes used by MCP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    /// Invalid JSON was received by the server (-32700).
    ParseError,
    /// The JSON sent is not a valid Request object (-32600).
    InvalidRequest,
    /// The method does not exist or is not available (-32601).
    MethodNotFound,
    /// Invalid method parameter(s) (-32602).
    InvalidParams,
    /// Internal JSON-RPC error (-32603).
    InternalError,
}

impl ErrorCode {
    /// Returns the numeric JSON-RPC error code.
    #[must_use]
    pub fn code(self) -> i64 {
        match self {
            Self::ParseError => -32700,
            Self::InvalidRequest => -32600,
            Self::MethodNotFound => -32601,
            Self::InvalidParams => -32602,
            Self::InternalError => -32603,
        }
    }

    /// Returns the standard message for this error code.
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            Self::ParseError => "Parse error",
            Self::InvalidRequest => "Invalid Request",
            Self::MethodNotFound => "Method not found",
            Self::InvalidParams => "Invalid params",
            Self::InternalError => "Internal error",
        }
    }
}

/// Error returned by tool handler functions.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// The parameters provided to the tool were invalid.
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    /// An error occurred during tool execution.
    #[error("Execution error: {0}")]
    ExecutionError(String),

    /// The tool encountered an internal error.
    #[error("Internal error: {0}")]
    Internal(String),

    /// A generic error with a custom message.
    #[error("{0}")]
    Other(String),
}

impl ToolError {
    /// Returns the JSON-RPC error code for this tool error.
    #[must_use]
    pub fn error_code(&self) -> ErrorCode {
        match self {
            Self::InvalidParams(_) => ErrorCode::InvalidParams,
            Self::ExecutionError(_) | Self::Internal(_) | Self::Other(_) => {
                ErrorCode::InternalError
            }
        }
    }
}

/// Error returned by resource handler functions.
#[derive(Debug, thiserror::Error)]
pub enum ResourceError {
    /// The resource was not found at the given URI.
    #[error("Resource not found: {0}")]
    NotFound(String),

    /// The URI provided was invalid or malformed.
    #[error("Invalid URI: {0}")]
    InvalidUri(String),

    /// An I/O error occurred while reading the resource.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A generic error with a custom message.
    #[error("{0}")]
    Other(String),
}

impl ResourceError {
    /// Returns the JSON-RPC error code for this resource error.
    #[must_use]
    pub fn error_code(&self) -> ErrorCode {
        match self {
            Self::NotFound(_) | Self::InvalidUri(_) => ErrorCode::InvalidParams,
            Self::Io(_) | Self::Other(_) => ErrorCode::InternalError,
        }
    }
}

/// Error returned by prompt handler functions.
#[derive(Debug, thiserror::Error)]
pub enum PromptError {
    /// The prompt arguments were invalid.
    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),

    /// An error occurred while generating the prompt.
    #[error("Generation error: {0}")]
    GenerationError(String),

    /// A generic error with a custom message.
    #[error("{0}")]
    Other(String),
}

impl PromptError {
    /// Returns the JSON-RPC error code for this prompt error.
    #[must_use]
    pub fn error_code(&self) -> ErrorCode {
        match self {
            Self::InvalidArguments(_) => ErrorCode::InvalidParams,
            Self::GenerationError(_) | Self::Other(_) => ErrorCode::InternalError,
        }
    }
}

/// Transport-level errors.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// An I/O error occurred in the transport layer.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Failed to serialize a message.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Failed to deserialize a message.
    #[error("Deserialization error: {0}")]
    Deserialization(String),

    /// The transport connection was closed.
    #[error("Connection closed")]
    ConnectionClosed,

    /// A generic transport error.
    #[error("{0}")]
    Other(String),
}

/// Top-level SDK error type.
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    /// A transport-level error occurred.
    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),

    /// A protocol-level error occurred.
    #[error("Protocol error: {0}")]
    Protocol(String),

    /// A tool execution error occurred.
    #[error("Tool error: {0}")]
    Tool(#[from] ToolError),

    /// A resource error occurred.
    #[error("Resource error: {0}")]
    Resource(#[from] ResourceError),

    /// A prompt error occurred.
    #[error("Prompt error: {0}")]
    Prompt(#[from] PromptError),

    /// A JSON-RPC error received from the protocol layer.
    #[error("JSON-RPC error {code}: {message}")]
    JsonRpc {
        /// The JSON-RPC error code.
        code: i64,
        /// The error message.
        message: String,
        /// Optional additional error data.
        data: Option<serde_json::Value>,
    },

    /// Server configuration error.
    #[error("Configuration error: {0}")]
    Configuration(String),
}

/// A JSON-RPC error object included in error responses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcError {
    /// The error code.
    pub code: i64,
    /// The error message.
    pub message: String,
    /// Optional additional data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcError {
    /// Creates a new JSON-RPC error from an error code with a custom detail message.
    #[must_use]
    pub fn new(code: ErrorCode, detail: Option<String>) -> Self {
        let message = if let Some(detail) = detail {
            format!("{}: {}", code.message(), detail)
        } else {
            code.message().to_string()
        };
        Self {
            code: code.code(),
            message,
            data: None,
        }
    }

    /// Creates a parse error.
    #[must_use]
    pub fn parse_error(detail: Option<String>) -> Self {
        Self::new(ErrorCode::ParseError, detail)
    }

    /// Creates an invalid request error.
    #[must_use]
    pub fn invalid_request(detail: Option<String>) -> Self {
        Self::new(ErrorCode::InvalidRequest, detail)
    }

    /// Creates a method not found error.
    #[must_use]
    pub fn method_not_found(detail: Option<String>) -> Self {
        Self::new(ErrorCode::MethodNotFound, detail)
    }

    /// Creates an invalid params error.
    #[must_use]
    pub fn invalid_params(detail: Option<String>) -> Self {
        Self::new(ErrorCode::InvalidParams, detail)
    }

    /// Creates an internal error.
    #[must_use]
    pub fn internal_error(detail: Option<String>) -> Self {
        Self::new(ErrorCode::InternalError, detail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes() {
        assert_eq!(ErrorCode::ParseError.code(), -32700);
        assert_eq!(ErrorCode::InvalidRequest.code(), -32600);
        assert_eq!(ErrorCode::MethodNotFound.code(), -32601);
        assert_eq!(ErrorCode::InvalidParams.code(), -32602);
        assert_eq!(ErrorCode::InternalError.code(), -32603);
    }

    #[test]
    fn test_error_messages() {
        assert_eq!(ErrorCode::ParseError.message(), "Parse error");
        assert_eq!(ErrorCode::InvalidRequest.message(), "Invalid Request");
        assert_eq!(ErrorCode::MethodNotFound.message(), "Method not found");
        assert_eq!(ErrorCode::InvalidParams.message(), "Invalid params");
        assert_eq!(ErrorCode::InternalError.message(), "Internal error");
    }

    #[test]
    fn test_jsonrpc_error_serialization() {
        let error = JsonRpcError::parse_error(Some("unexpected token".to_string()));
        let json = serde_json::to_value(&error).expect("serialize");
        assert_eq!(json["code"], -32700);
        assert!(json["message"].as_str().unwrap().contains("Parse error"));
    }

    #[test]
    fn test_tool_error_codes() {
        let err = ToolError::InvalidParams("bad input".to_string());
        assert_eq!(err.error_code(), ErrorCode::InvalidParams);

        let err = ToolError::ExecutionError("boom".to_string());
        assert_eq!(err.error_code(), ErrorCode::InternalError);
    }
}
