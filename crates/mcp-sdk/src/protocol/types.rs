//! JSON-RPC 2.0 type definitions for MCP.
//!
//! This module provides the core JSON-RPC 2.0 types used by the Model Context
//! Protocol. All MCP communication is built on JSON-RPC 2.0 messages.

use serde::{Deserialize, Serialize};

use crate::types::error::JsonRpcError;

/// The JSON-RPC version string, always "2.0".
pub const JSONRPC_VERSION: &str = "2.0";

/// A JSON-RPC request or response ID.
///
/// Can be a number or string, as per the JSON-RPC 2.0 specification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum RequestId {
    /// A numeric request ID.
    Number(i64),
    /// A string request ID.
    String(String),
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Number(n) => write!(f, "{n}"),
            Self::String(s) => write!(f, "{s}"),
        }
    }
}

/// A JSON-RPC 2.0 message, which can be a request, response, or notification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    /// A JSON-RPC request (has `id` and `method`).
    Request(JsonRpcRequest),
    /// A JSON-RPC successful response (has `id` and `result`).
    Response(JsonRpcResponse),
    /// A JSON-RPC error response (has `id` and `error`).
    ErrorResponse(JsonRpcErrorResponse),
    /// A JSON-RPC notification (has `method` but no `id`).
    Notification(JsonRpcNotification),
}

impl JsonRpcMessage {
    /// Creates a new JSON-RPC request.
    #[must_use]
    pub fn request(
        id: RequestId,
        method: impl Into<String>,
        params: Option<serde_json::Value>,
    ) -> Self {
        Self::Request(JsonRpcRequest {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            method: method.into(),
            params,
        })
    }

    /// Creates a new JSON-RPC successful response.
    #[must_use]
    pub fn response(id: RequestId, result: serde_json::Value) -> Self {
        Self::Response(JsonRpcResponse {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result,
        })
    }

    /// Creates a new JSON-RPC error response.
    #[must_use]
    pub fn error_response(id: RequestId, error: JsonRpcError) -> Self {
        Self::ErrorResponse(JsonRpcErrorResponse {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            error,
        })
    }

    /// Creates a new JSON-RPC notification (no `id`).
    #[must_use]
    pub fn notification(method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self::Notification(JsonRpcNotification {
            jsonrpc: JSONRPC_VERSION.to_string(),
            method: method.into(),
            params,
        })
    }

    /// Returns the request ID, if this is a request or response.
    #[must_use]
    pub fn id(&self) -> Option<&RequestId> {
        match self {
            Self::Request(r) => Some(&r.id),
            Self::Response(r) => Some(&r.id),
            Self::ErrorResponse(r) => Some(&r.id),
            Self::Notification(_) => None,
        }
    }

    /// Returns the method name, if this is a request or notification.
    #[must_use]
    pub fn method(&self) -> Option<&str> {
        match self {
            Self::Request(r) => Some(&r.method),
            Self::Notification(n) => Some(&n.method),
            Self::Response(_) | Self::ErrorResponse(_) => None,
        }
    }
}

/// A JSON-RPC 2.0 request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcRequest {
    /// The JSON-RPC version, always "2.0".
    pub jsonrpc: String,
    /// The request ID.
    pub id: RequestId,
    /// The method name.
    pub method: String,
    /// Optional parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// A JSON-RPC 2.0 successful response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcResponse {
    /// The JSON-RPC version, always "2.0".
    pub jsonrpc: String,
    /// The request ID this response corresponds to.
    pub id: RequestId,
    /// The result value.
    pub result: serde_json::Value,
}

/// A JSON-RPC 2.0 error response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcErrorResponse {
    /// The JSON-RPC version, always "2.0".
    pub jsonrpc: String,
    /// The request ID this error response corresponds to.
    pub id: RequestId,
    /// The error object.
    pub error: JsonRpcError,
}

/// A JSON-RPC 2.0 notification (no `id`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcNotification {
    /// The JSON-RPC version, always "2.0".
    pub jsonrpc: String,
    /// The method name.
    pub method: String,
    /// Optional parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// Parses a raw JSON string into a `JsonRpcMessage`.
///
/// This function handles the detection of whether the message is a request,
/// response, error response, or notification based on the presence of
/// `id`, `method`, `result`, and `error` fields.
///
/// # Errors
///
/// Returns an error string if the JSON is invalid or doesn't conform to
/// JSON-RPC 2.0 structure.
pub fn parse_jsonrpc_message(json: &str) -> Result<JsonRpcMessage, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("Invalid JSON: {e}"))?;

    let obj = value
        .as_object()
        .ok_or_else(|| "JSON-RPC message must be an object".to_string())?;

    // Validate jsonrpc field
    match obj.get("jsonrpc") {
        Some(v) if v == "2.0" => {}
        Some(_) => return Err("jsonrpc field must be \"2.0\"".to_string()),
        None => return Err("Missing jsonrpc field".to_string()),
    }

    let has_id = obj.contains_key("id");
    let has_method = obj.contains_key("method");
    let has_result = obj.contains_key("result");
    let has_error = obj.contains_key("error");

    if has_id && has_method && !has_result && !has_error {
        // Request
        let request: JsonRpcRequest =
            serde_json::from_value(value).map_err(|e| format!("Invalid request: {e}"))?;
        Ok(JsonRpcMessage::Request(request))
    } else if has_id && has_result && !has_error {
        // Response
        let response: JsonRpcResponse =
            serde_json::from_value(value).map_err(|e| format!("Invalid response: {e}"))?;
        Ok(JsonRpcMessage::Response(response))
    } else if has_id && has_error {
        // Error response
        let error_response: JsonRpcErrorResponse =
            serde_json::from_value(value).map_err(|e| format!("Invalid error response: {e}"))?;
        Ok(JsonRpcMessage::ErrorResponse(error_response))
    } else if has_method && !has_id {
        // Notification
        let notification: JsonRpcNotification =
            serde_json::from_value(value).map_err(|e| format!("Invalid notification: {e}"))?;
        Ok(JsonRpcMessage::Notification(notification))
    } else {
        Err("Unrecognized JSON-RPC 2.0 message structure".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_request() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#;
        let msg = parse_jsonrpc_message(json).expect("should parse");
        match msg {
            JsonRpcMessage::Request(req) => {
                assert_eq!(req.id, RequestId::Number(1));
                assert_eq!(req.method, "tools/list");
            }
            _ => panic!("Expected request"),
        }
    }

    #[test]
    fn test_parse_notification() {
        let json = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        let msg = parse_jsonrpc_message(json).expect("should parse");
        match msg {
            JsonRpcMessage::Notification(notif) => {
                assert_eq!(notif.method, "notifications/initialized");
                assert!(notif.params.is_none());
            }
            _ => panic!("Expected notification"),
        }
    }

    #[test]
    fn test_parse_response() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#;
        let msg = parse_jsonrpc_message(json).expect("should parse");
        match msg {
            JsonRpcMessage::Response(resp) => {
                assert_eq!(resp.id, RequestId::Number(1));
            }
            _ => panic!("Expected response"),
        }
    }

    #[test]
    fn test_parse_error_response() {
        let json =
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
        let msg = parse_jsonrpc_message(json).expect("should parse");
        match msg {
            JsonRpcMessage::ErrorResponse(err) => {
                assert_eq!(err.error.code, -32601);
                assert_eq!(err.error.message, "Method not found");
            }
            _ => panic!("Expected error response"),
        }
    }

    #[test]
    fn test_parse_invalid_json() {
        let result = parse_jsonrpc_message("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_jsonrpc() {
        let json = r#"{"id":1,"method":"test"}"#;
        let result = parse_jsonrpc_message(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing jsonrpc"));
    }

    #[test]
    fn test_parse_wrong_version() {
        let json = r#"{"jsonrpc":"1.0","id":1,"method":"test"}"#;
        let result = parse_jsonrpc_message(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_request_id_string() {
        let json = r#"{"jsonrpc":"2.0","id":"abc-123","method":"ping"}"#;
        let msg = parse_jsonrpc_message(json).expect("should parse");
        match msg {
            JsonRpcMessage::Request(req) => {
                assert_eq!(req.id, RequestId::String("abc-123".to_string()));
            }
            _ => panic!("Expected request"),
        }
    }

    #[test]
    fn test_message_constructors() {
        let req = JsonRpcMessage::request(RequestId::Number(1), "test", None);
        assert_eq!(req.id(), Some(&RequestId::Number(1)));
        assert_eq!(req.method(), Some("test"));

        let notif = JsonRpcMessage::notification("test", None);
        assert!(notif.id().is_none());
        assert_eq!(notif.method(), Some("test"));
    }

    #[test]
    fn test_roundtrip_serialization() {
        let req = JsonRpcRequest {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: RequestId::Number(42),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({"name": "add", "arguments": {"a": 1, "b": 2}})),
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let back: JsonRpcRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(req, back);
    }
}
