//! Conformance tests against the MCP 2025-11-25 specification.
//!
//! Each test cites the spec text it checks, as
//! `<file>:<line>` in modelcontextprotocol/modelcontextprotocol at
//! `docs/specification/2025-11-25/`.

use std::time::Duration;

use async_trait::async_trait;
use mcp_sdk::context::ToolContext;
use mcp_sdk::handler::tools::ToolHandler;
use mcp_sdk::protocol::messages::ToolInfo;
use mcp_sdk::protocol::types::JsonRpcMessage;
use mcp_sdk::server::McpServer;
use mcp_sdk::transport::stdio::TestStdioTransport;
use mcp_sdk::transport::McpTransport;
use mcp_sdk::types::content::CallToolResult;
use mcp_sdk::types::error::{ToolError, TransportError};
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

struct Echo;

#[async_trait]
impl ToolHandler for Echo {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "echo".to_string(),
            description: None,
            input_schema: json!({
                "type": "object",
                "properties": {"message": {"type": "string"}},
                "required": ["message"]
            }),
            annotations: None,
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

struct Fail;

#[async_trait]
impl ToolHandler for Fail {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "fail".to_string(),
            description: None,
            input_schema: json!({"type": "object"}),
            annotations: None,
        }
    }

    async fn call(
        &self,
        _arguments: Value,
        _context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        Err(ToolError::ExecutionError("boom".to_string()))
    }
}

struct Panics;

#[async_trait]
impl ToolHandler for Panics {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "panics".to_string(),
            description: None,
            input_schema: json!({"type": "object"}),
            annotations: None,
        }
    }

    async fn call(
        &self,
        _arguments: Value,
        _context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        panic!("tool panicked");
    }
}

/// Sleeps for `ms`, ignoring cancellation, so tests can tell whether the
/// server (not the tool) honours `notifications/cancelled`.
struct Slow;

#[async_trait]
impl ToolHandler for Slow {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "slow".to_string(),
            description: None,
            input_schema: json!({"type": "object", "properties": {"ms": {"type": "integer"}}}),
            annotations: None,
        }
    }

    async fn call(
        &self,
        arguments: Value,
        _context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        let ms = arguments.get("ms").and_then(Value::as_u64).unwrap_or(0);
        tokio::time::sleep(Duration::from_millis(ms)).await;
        Ok(CallToolResult::text("done"))
    }
}

fn server() -> McpServer {
    McpServer::builder()
        .name("conformance")
        .version("0.0.0")
        .tool(Echo)
        .tool(Fail)
        .tool(Panics)
        .tool(Slow)
        .build()
}

const INIT: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
"#;

/// Runs the server over newline-delimited input and returns every message
/// it wrote, in order.
async fn run_lines(input: &str) -> Vec<Value> {
    let transport = TestStdioTransport::from_input(input);
    let output = transport.output_handle();
    server().run(transport).await.expect("server run");
    let bytes = output.lock().await.clone();
    String::from_utf8(bytes)
        .expect("utf8")
        .lines()
        .map(|l| serde_json::from_str(l).expect("server wrote valid JSON"))
        .collect()
}

fn by_id(messages: &[Value], id: i64) -> &Value {
    messages
        .iter()
        .find(|m| m.get("id") == Some(&json!(id)))
        .unwrap_or_else(|| panic!("no message with id {id} in {messages:#?}"))
}

/// A transport fed from a channel, so tests can interleave messages with a
/// running server and observe response timing.
struct ChannelTransport {
    incoming: mpsc::UnboundedReceiver<JsonRpcMessage>,
    outgoing: mpsc::UnboundedSender<JsonRpcMessage>,
}

#[async_trait]
impl McpTransport for ChannelTransport {
    async fn recv(&mut self) -> Result<Option<JsonRpcMessage>, TransportError> {
        Ok(self.incoming.recv().await)
    }

    async fn send(&self, message: &JsonRpcMessage) -> Result<(), TransportError> {
        self.outgoing
            .send(message.clone())
            .map_err(|_| TransportError::ConnectionClosed)
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

struct Live {
    to_server: mpsc::UnboundedSender<JsonRpcMessage>,
    from_server: Mutex<mpsc::UnboundedReceiver<JsonRpcMessage>>,
}

impl Live {
    async fn start() -> Self {
        let (to_server, incoming) = mpsc::unbounded_channel();
        let (outgoing, from_server) = mpsc::unbounded_channel();
        tokio::spawn(server().run(ChannelTransport { incoming, outgoing }));
        let live = Self {
            to_server,
            from_server: Mutex::new(from_server),
        };
        for line in INIT.lines() {
            live.send(serde_json::from_str(line).unwrap());
        }
        live.next(Duration::from_secs(1))
            .await
            .expect("initialize response");
        live
    }

    fn send(&self, message: Value) {
        self.to_server
            .send(serde_json::from_value(message).unwrap())
            .unwrap();
    }

    async fn next(&self, within: Duration) -> Option<Value> {
        let mut rx = self.from_server.lock().await;
        tokio::time::timeout(within, rx.recv())
            .await
            .ok()
            .flatten()
            .map(|m| serde_json::to_value(m).unwrap())
    }
}

fn call(id: i64, name: &str, arguments: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": arguments}})
        .to_string()
}

// ---------------------------------------------------------------------------
// Tools: error reporting (server/tools.mdx:460-477)
// ---------------------------------------------------------------------------

/// server/tools.mdx:469-473 - API failures and business logic errors are
/// "Tool Execution Errors: Reported in tool results with `isError: true`".
#[tokio::test]
async fn tool_execution_error_is_an_is_error_result() {
    let out = run_lines(&format!("{INIT}{}\n", call(2, "fail", json!({})))).await;
    let resp = by_id(&out, 2);
    assert!(resp.get("error").is_none(), "expected a result, got {resp}");
    assert_eq!(resp["result"]["isError"], true);
    assert_eq!(resp["result"]["content"][0]["text"], "boom");
}

/// server/tools.mdx:471 - "Input validation errors (e.g., date in wrong
/// format, value out of range)" are tool execution errors, not protocol errors.
#[tokio::test]
async fn tool_input_validation_error_is_an_is_error_result() {
    let out = run_lines(&format!(
        "{INIT}{}\n",
        call(2, "echo", json!({"message": 42}))
    ))
    .await;
    let resp = by_id(&out, 2);
    assert!(resp.get("error").is_none(), "expected a result, got {resp}");
    assert_eq!(resp["result"]["isError"], true);
}

/// server/tools.mdx:464-466,486 - unknown tools are protocol errors (-32602).
#[tokio::test]
async fn unknown_tool_is_an_invalid_params_protocol_error() {
    let out = run_lines(&format!("{INIT}{}\n", call(2, "nope", json!({})))).await;
    assert_eq!(by_id(&out, 2)["error"]["code"], -32602);
}

// ---------------------------------------------------------------------------
// JSON-RPC framing (basic/index.mdx:85-91; JSON-RPC 2.0 section 5.1)
// ---------------------------------------------------------------------------

/// basic/index.mdx:89 - when the id "could not be read due a malformed
/// request" the error response carries no id (schema.ts `id?: RequestId`).
/// Invalid JSON is -32700 Parse error.
#[tokio::test]
async fn invalid_json_gets_a_parse_error_without_an_id() {
    let out = run_lines(&format!("{INIT}{{not json\n")).await;
    let err = out
        .iter()
        .find(|m| m.get("error").is_some())
        .expect("an error response");
    assert_eq!(err["error"]["code"], -32700);
    assert!(err.get("id").is_none(), "id must be omitted, got {err}");
}

/// JSON-RPC 2.0 section 5.1 - valid JSON that is not a valid Request object
/// is -32600 Invalid Request, again without an id.
#[tokio::test]
async fn non_request_json_gets_invalid_request_without_an_id() {
    for line in [
        r#"{"foo":1}"#,
        r#"{"jsonrpc":"1.0","id":20,"method":"ping"}"#,
        r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#,
        r#"[{"jsonrpc":"2.0","id":21,"method":"ping"}]"#,
    ] {
        let out = run_lines(&format!("{INIT}{line}\n")).await;
        let err = out
            .iter()
            .find(|m| m.get("error").is_some())
            .expect("an error response");
        assert_eq!(err["error"]["code"], -32600, "for input {line}");
        assert!(
            err.get("id").is_none(),
            "id must be omitted for {line}, got {err}"
        );
    }
}

/// basic/index.mdx:89 - a failure inside a request handler is still answered
/// with that request's id.
#[tokio::test]
async fn panicking_tool_is_answered_with_its_own_request_id() {
    let out = run_lines(&format!("{INIT}{}\n", call(7, "panics", json!({})))).await;
    assert_eq!(by_id(&out, 7)["error"]["code"], -32603);
}

// ---------------------------------------------------------------------------
// Lifecycle and ping
// ---------------------------------------------------------------------------

/// basic/lifecycle.mdx:157-159 - clients may send pings before the server has
/// answered `initialize`, and basic/utilities/ping.mdx:29 - "The receiver
/// MUST respond promptly with an empty response".
#[tokio::test]
async fn ping_before_initialize_is_answered() {
    let out = run_lines("{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}\n").await;
    assert_eq!(by_id(&out, 7)["result"], json!({}));
}

/// basic/utilities/ping.mdx:29 - ping is answered promptly even while a tool
/// call is still running.
#[tokio::test]
async fn ping_is_answered_while_a_tool_call_runs() {
    let live = Live::start().await;
    live.send(serde_json::from_str(&call(2, "slow", json!({"ms": 2000}))).unwrap());
    live.send(json!({"jsonrpc": "2.0", "id": 3, "method": "ping"}));
    let first = live.next(Duration::from_millis(500)).await;
    assert_eq!(
        first.map(|m| m["id"].clone()),
        Some(json!(3)),
        "ping must not wait for the tool"
    );
}

// ---------------------------------------------------------------------------
// Cancellation (basic/utilities/cancellation.mdx:37-40)
// ---------------------------------------------------------------------------

/// basic/utilities/cancellation.mdx:37-40 - receivers SHOULD stop processing
/// the cancelled request and SHOULD NOT send a response for it.
#[tokio::test]
async fn cancelled_request_gets_no_response() {
    let live = Live::start().await;
    live.send(serde_json::from_str(&call(2, "slow", json!({"ms": 800}))).unwrap());
    live.send(json!({
        "jsonrpc": "2.0",
        "method": "notifications/cancelled",
        "params": {"requestId": 2, "reason": "test"}
    }));
    live.send(json!({"jsonrpc": "2.0", "id": 3, "method": "ping"}));
    assert_eq!(
        live.next(Duration::from_millis(500))
            .await
            .map(|m| m["id"].clone()),
        Some(json!(3))
    );
    assert_eq!(
        live.next(Duration::from_millis(1200)).await,
        None,
        "no response for a cancelled request"
    );
}
