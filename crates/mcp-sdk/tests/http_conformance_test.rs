//! Streamable HTTP conformance tests against the MCP 2025-11-25 specification.
//!
//! Spec citations are `basic/transports.mdx:<line>` in
//! modelcontextprotocol/modelcontextprotocol at
//! `docs/specification/2025-11-25/`.

#![cfg(feature = "http")]

use std::time::Duration;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Request, StatusCode};
use mcp_sdk::context::ToolContext;
use mcp_sdk::handler::tools::ToolHandler;
use mcp_sdk::protocol::messages::ToolInfo;
use mcp_sdk::server::McpServer;
use mcp_sdk::transport::http::{HttpTransport, HttpTransportBuilder};
use mcp_sdk::types::content::CallToolResult;
use mcp_sdk::types::error::ToolError;
use serde_json::{json, Value};
use tower::ServiceExt;

/// Echoes `message` after sleeping `ms` milliseconds.
struct Echo;

#[async_trait]
impl ToolHandler for Echo {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "echo".to_string(),
            description: None,
            input_schema: json!({"type": "object"}),
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
        Ok(CallToolResult::text(
            arguments["message"].as_str().unwrap_or_default(),
        ))
    }
}

fn start(builder: HttpTransportBuilder) -> axum::Router {
    let transport = builder.build();
    let router = transport.router();
    let server = McpServer::builder()
        .name("http-conformance")
        .tool(Echo)
        .build();
    tokio::spawn(server.run(transport));
    router
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or_else(|e| panic!("{e}: {:?}", self.body))
    }
}

async fn send(
    router: &axum::Router,
    method: &str,
    headers: &[(&str, &str)],
    body: Option<Value>,
) -> Reply {
    let mut req = Request::builder()
        .method(method)
        .uri("/mcp")
        .body(
            body.as_ref()
                .map_or_else(Body::empty, |b| Body::from(b.to_string())),
        )
        .unwrap();
    let h = req.headers_mut();
    h.insert("host", HeaderValue::from_static("127.0.0.1:8090"));
    h.insert(
        "accept",
        HeaderValue::from_static("application/json, text/event-stream"),
    );
    if body.is_some() {
        h.insert("content-type", HeaderValue::from_static("application/json"));
    }
    for (k, v) in headers {
        h.insert(
            HeaderName::from_bytes(k.as_bytes()).unwrap(),
            HeaderValue::from_str(v).unwrap(),
        );
    }
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    Reply {
        status,
        headers,
        body: String::from_utf8(bytes.to_vec()).unwrap(),
    }
}

fn initialize() -> Value {
    json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}
    })
}

fn ping(id: i64) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "ping"})
}

/// Runs the initialize handshake and returns the session id.
async fn open_session(router: &axum::Router) -> String {
    let reply = send(router, "POST", &[], Some(initialize())).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let sid = reply
        .headers
        .get("mcp-session-id")
        .expect("initialize response must carry Mcp-Session-Id")
        .to_str()
        .unwrap()
        .to_string();
    let initialized = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    let reply = send(
        router,
        "POST",
        &[
            ("mcp-session-id", &sid),
            ("mcp-protocol-version", "2025-11-25"),
        ],
        Some(initialized),
    )
    .await;
    assert_eq!(reply.status, StatusCode::ACCEPTED);
    sid
}

// ---------------------------------------------------------------------------
// Sending messages (transports.mdx:86-104)
// ---------------------------------------------------------------------------

/// transports.mdx:95-97 - for an accepted notification or response "the
/// server MUST return HTTP status code 202 Accepted with no body".
#[tokio::test]
async fn accepted_notification_gets_202_with_no_body() {
    let router = start(HttpTransport::builder().enable_sessions(false));
    let reply = send(
        &router,
        "POST",
        &[],
        Some(json!({"jsonrpc": "2.0", "method": "notifications/initialized"})),
    )
    .await;
    assert_eq!(reply.status, StatusCode::ACCEPTED);
    assert_eq!(reply.body, "", "202 must have no body");
}

/// transports.mdx:98-100 - an input the server cannot accept gets an HTTP
/// error status, and the body MAY be "a JSON-RPC error response that has no
/// id" (schema.ts JSONRPCErrorResponse `id?: RequestId`, never null).
#[tokio::test]
async fn invalid_message_error_body_has_no_id() {
    let router = start(HttpTransport::builder().enable_sessions(false));
    let reply = send(
        &router,
        "POST",
        &[],
        Some(json!({"jsonrpc": "1.0", "id": 5, "method": "ping"})),
    )
    .await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    let body = reply.json();
    assert_eq!(body["error"]["code"], -32600);
    assert!(body.get("id").is_none(), "id must be absent, got {body}");
}

// ---------------------------------------------------------------------------
// Security (transports.mdx:74-84)
// ---------------------------------------------------------------------------

/// transports.mdx:78-80 - "If the Origin header is present and invalid,
/// servers MUST respond with HTTP 403 Forbidden".
#[tokio::test]
async fn foreign_origin_is_forbidden() {
    let router = start(HttpTransport::builder().enable_sessions(false));
    let reply = send(
        &router,
        "POST",
        &[("origin", "http://evil.example")],
        Some(ping(1)),
    )
    .await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
}

/// transports.mdx:78 - DNS rebinding: a request whose Host is not the local
/// server is rejected.
#[tokio::test]
async fn foreign_host_is_forbidden() {
    let router = start(HttpTransport::builder().enable_sessions(false));
    let reply = send(&router, "POST", &[("host", "evil.example")], Some(ping(1))).await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
}

/// Local origins and hosts keep working.
#[tokio::test]
async fn localhost_origin_is_allowed() {
    let router = start(HttpTransport::builder().enable_sessions(false));
    for origin in [
        "http://localhost:6274",
        "http://127.0.0.1",
        "http://[::1]:3000",
    ] {
        let reply = send(&router, "POST", &[("origin", origin)], Some(ping(1))).await;
        assert_eq!(
            reply.status,
            StatusCode::OK,
            "origin {origin}: {}",
            reply.body
        );
    }
}

/// Deployments behind a proxy can allow their public host name.
#[tokio::test]
async fn configured_host_is_allowed() {
    let router = start(
        HttpTransport::builder()
            .enable_sessions(false)
            .allowed_hosts(["mcp.example.com"]),
    );
    let reply = send(
        &router,
        "POST",
        &[
            ("host", "mcp.example.com"),
            ("origin", "https://mcp.example.com"),
        ],
        Some(ping(1)),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
}

/// transports.mdx:81 - "When running locally, servers SHOULD bind only to
/// localhost (127.0.0.1) rather than all network interfaces (0.0.0.0)".
#[test]
fn default_bind_address_is_localhost() {
    assert!(HttpTransport::new().bind_addr().ip().is_loopback());
}

// ---------------------------------------------------------------------------
// Sessions (transports.mdx:192-220)
// ---------------------------------------------------------------------------

/// transports.mdx:198-200 - the session id is returned "in an MCP-Session-Id
/// header on the HTTP response containing the InitializeResult".
#[tokio::test]
async fn initialize_response_carries_session_id() {
    let router = start(HttpTransport::builder());
    let sid = open_session(&router).await;
    assert!(
        sid.bytes().all(|b| (0x21..=0x7e).contains(&b)),
        "transports.mdx:203-204"
    );
}

/// transports.mdx:209-210 - servers that require a session id SHOULD answer
/// requests without one (other than initialize) with 400.
#[tokio::test]
async fn request_without_session_id_is_rejected() {
    let router = start(HttpTransport::builder());
    open_session(&router).await;
    let reply = send(
        &router,
        "POST",
        &[("mcp-protocol-version", "2025-11-25")],
        Some(ping(2)),
    )
    .await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
}

/// transports.mdx:211-212 and 216-218 - after DELETE the session is gone and
/// requests carrying it get 404.
#[tokio::test]
async fn deleted_session_gets_404() {
    let router = start(HttpTransport::builder());
    let sid = open_session(&router).await;
    let reply = send(&router, "DELETE", &[("mcp-session-id", &sid)], None).await;
    assert!(reply.status.is_success(), "{}", reply.status);
    let reply = send(
        &router,
        "POST",
        &[
            ("mcp-session-id", &sid),
            ("mcp-protocol-version", "2025-11-25"),
        ],
        Some(ping(2)),
    )
    .await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
}

/// basic/index.mdx:69 - "Result responses MUST include the same ID as the
/// request they correspond to". Two sessions may both use id 1; each must
/// get its own response.
#[tokio::test]
async fn same_request_id_in_two_sessions_is_routed_to_each_session() {
    let router = start(HttpTransport::builder());
    let a = open_session(&router).await;
    let b = open_session(&router).await;
    let call = |message: &str, ms: u64| {
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
               "params": {"name": "echo", "arguments": {"message": message, "ms": ms}}})
    };
    let pv = ("mcp-protocol-version", "2025-11-25");
    let headers_a = [("mcp-session-id", a.as_str()), pv];
    let headers_b = [("mcp-session-id", b.as_str()), pv];
    let (ra, rb) = tokio::join!(
        send(&router, "POST", &headers_a, Some(call("from-A", 300))),
        async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            send(&router, "POST", &headers_b, Some(call("from-B", 0))).await
        },
    );
    assert_eq!(
        ra.json()["result"]["content"][0]["text"],
        "from-A",
        "{}",
        ra.body
    );
    assert_eq!(
        rb.json()["result"]["content"][0]["text"],
        "from-B",
        "{}",
        rb.body
    );
    assert_eq!(ra.json()["id"], 1);
    assert_eq!(rb.json()["id"], 1);
}

// ---------------------------------------------------------------------------
// Protocol version header (transports.mdx:263-280)
// ---------------------------------------------------------------------------

/// transports.mdx:279-280 - "If the server receives a request with an invalid
/// or unsupported MCP-Protocol-Version, it MUST respond with 400 Bad Request".
#[tokio::test]
async fn unsupported_protocol_version_header_is_400() {
    let router = start(HttpTransport::builder());
    let sid = open_session(&router).await;
    let reply = send(
        &router,
        "POST",
        &[
            ("mcp-session-id", &sid),
            ("mcp-protocol-version", "1999-01-01"),
        ],
        Some(ping(2)),
    )
    .await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
}

/// transports.mdx:274-277 - without the header the server falls back to an
/// assumed version rather than rejecting the request.
#[tokio::test]
async fn missing_protocol_version_header_is_accepted() {
    let router = start(HttpTransport::builder());
    let sid = open_session(&router).await;
    let reply = send(&router, "POST", &[("mcp-session-id", &sid)], Some(ping(2))).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
}

// ---------------------------------------------------------------------------
// Cancellation (basic/utilities/cancellation.mdx:37-40)
// ---------------------------------------------------------------------------

/// cancellation.mdx:37-40 - a cancelled request gets no JSON-RPC response, so
/// its POST is released promptly with an empty body instead of waiting for
/// the transport's response timeout.
#[tokio::test]
async fn cancelled_request_post_is_released_without_a_response() {
    let router = start(HttpTransport::builder());
    let sid = open_session(&router).await;
    let headers = [
        ("mcp-session-id", sid.as_str()),
        ("mcp-protocol-version", "2025-11-25"),
    ];
    let slow = json!({"jsonrpc": "2.0", "id": 9, "method": "tools/call",
                      "params": {"name": "echo", "arguments": {"message": "late", "ms": 5000}}});
    let cancel = json!({"jsonrpc": "2.0", "method": "notifications/cancelled",
                        "params": {"requestId": 9, "reason": "test"}});
    let (call, cancelled) = tokio::join!(
        tokio::time::timeout(
            Duration::from_secs(2),
            send(&router, "POST", &headers, Some(slow))
        ),
        async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            send(&router, "POST", &headers, Some(cancel)).await
        },
    );
    assert_eq!(cancelled.status, StatusCode::ACCEPTED);
    let call = call.expect("cancelled POST must not wait for the response timeout");
    assert_eq!(
        call.body, "",
        "no JSON-RPC response for a cancelled request"
    );
}
