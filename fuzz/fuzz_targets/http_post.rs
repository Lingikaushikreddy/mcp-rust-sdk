//! Arbitrary POST bodies and MCP headers never crash the HTTP transport or
//! hang the request, every non-empty body is a valid JSON-RPC message, and
//! 202 responses have no body.

#![no_main]

mod common;

use std::sync::OnceLock;
use std::time::Duration;

use arbitrary::Arbitrary;
use axum::body::Body;
use axum::http::{HeaderValue, Request};
use libfuzzer_sys::fuzz_target;
use mcp_sdk::protocol::types::decode_jsonrpc_message;
use mcp_sdk::transport::http::HttpTransport;
use tower::ServiceExt;

#[derive(Arbitrary, Debug)]
struct Input {
    body: Vec<u8>,
    session_id: Option<String>,
    protocol_version: Option<String>,
    origin: Option<String>,
    host: Option<String>,
}

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap())
}

fn router() -> &'static axum::Router {
    static ROUTER: OnceLock<axum::Router> = OnceLock::new();
    ROUTER.get_or_init(|| {
        let transport = HttpTransport::new();
        let router = transport.router();
        runtime().spawn(common::server().run(transport));
        router
    })
}

fuzz_target!(|input: Input| {
    let mut request = Request::builder()
        .method("POST")
        .uri("/mcp")
        .body(Body::from(input.body))
        .unwrap();
    let headers = request.headers_mut();
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    headers.insert("host", HeaderValue::from_static("127.0.0.1"));
    for (name, value) in [
        ("mcp-session-id", input.session_id),
        ("mcp-protocol-version", input.protocol_version),
        ("origin", input.origin),
        ("host", input.host),
    ] {
        if let Some(value) = value.and_then(|v| HeaderValue::from_str(&v).ok()) {
            headers.insert(name, value);
        }
    }

    runtime().block_on(async {
        let response = tokio::time::timeout(Duration::from_secs(5), router().clone().oneshot(request))
            .await
            .expect("POST must not hang")
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 22).await.unwrap();
        if status == 202 {
            assert!(bytes.is_empty(), "202 must have no body");
        } else if !bytes.is_empty() {
            let text = std::str::from_utf8(&bytes).expect("UTF-8 body");
            decode_jsonrpc_message(text).unwrap_or_else(|e| panic!("invalid body {text:?}: {e}"));
        }
    });
});
