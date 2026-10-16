//! Replays wire sessions recorded from real MCP clients and checks that the
//! server still answers exactly as it did when they were recorded.
//!
//! `tests/wire/*.jsonl` were captured with the MCP Inspector CLI (2.11.0)
//! and Claude Code (2.1.301) driving this fixture over stdio and Streamable
//! HTTP. See `tests/wire/README.md` for how to re-record them.

use std::collections::HashMap;

use axum::body::Body;
use axum::http::{HeaderName, HeaderValue, Request};
use mcp_example_conformance::build_server;
use mcp_sdk::transport::http::HttpTransport;
use mcp_sdk::transport::stdio::TestStdioTransport;
use serde_json::Value;
use tower::ServiceExt;

fn read_jsonl(name: &str) -> Vec<Value> {
    let path = format!("{}/tests/wire/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{path}: {e}"))
        .lines()
        .map(|l| serde_json::from_str(l).expect("valid JSONL"))
        .collect()
}

/// Splits a stdio recording into one entry list per client process. Each
/// process starts with `initialize`, optionally preceded by a
/// `server/discover` probe.
fn stdio_sessions(entries: Vec<Value>) -> Vec<Vec<Value>> {
    let mut sessions: Vec<Vec<Value>> = Vec::new();
    let mut seen_initialize = false;
    for entry in entries {
        let method = entry["msg"]["method"].as_str().unwrap_or_default();
        let starts_session = entry["dir"] == "c2s"
            && (sessions.is_empty()
                || (method == "initialize" && seen_initialize)
                || method == "server/discover");
        if starts_session {
            sessions.push(Vec::new());
            seen_initialize = false;
        }
        if entry["dir"] == "c2s" && method == "initialize" {
            seen_initialize = true;
        }
        sessions.last_mut().expect("first entry is c2s").push(entry);
    }
    sessions
}

fn key(message: &Value) -> String {
    message
        .get("id")
        .map_or_else(|| "<no id>".to_string(), ToString::to_string)
}

async fn replay_stdio(name: &str) {
    for (n, session) in stdio_sessions(read_jsonl(name)).into_iter().enumerate() {
        let input: String = session
            .iter()
            .filter(|e| e["dir"] == "c2s")
            .map(|e| format!("{}\n", e["msg"]))
            .collect();
        let expected: HashMap<String, Value> = session
            .iter()
            .filter(|e| e["dir"] == "s2c")
            .map(|e| (key(&e["msg"]), e["msg"].clone()))
            .collect();

        let transport = TestStdioTransport::from_input(&input);
        let output = transport.output_handle();
        build_server().run(transport).await.expect("server run");
        let bytes = output.lock().await.clone();
        let actual: HashMap<String, Value> = String::from_utf8(bytes)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .map(|m| (key(&m), m))
            .collect();

        // Requests run concurrently, so compare responses by id, not order.
        assert_eq!(actual, expected, "{name}, session {n}");
    }
}

async fn replay_http(name: &str) {
    let transport = HttpTransport::new();
    let router = transport.router();
    tokio::spawn(build_server().run(transport));

    // Recorded session ids map to the ids this run hands out.
    let mut sessions: HashMap<String, String> = HashMap::new();
    for (n, exchange) in read_jsonl(name).into_iter().enumerate() {
        let body = match exchange["c2s"].as_array().map(Vec::as_slice) {
            Some([message]) => Body::from(message.to_string()),
            _ => Body::empty(),
        };
        let mut request = Request::builder()
            .method(exchange["method"].as_str().unwrap())
            .uri("/mcp")
            .body(body)
            .unwrap();
        let headers = request.headers_mut();
        headers.insert("host", HeaderValue::from_static("127.0.0.1"));
        for (k, v) in exchange["req_headers"].as_object().unwrap() {
            let mut v = v.as_str().unwrap().to_string();
            if k == "mcp-session-id" {
                v = sessions.get(&v).cloned().unwrap_or(v);
            }
            headers.insert(
                HeaderName::from_bytes(k.as_bytes()).unwrap(),
                HeaderValue::from_str(&v).unwrap(),
            );
        }

        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(
            response.status().as_u16(),
            exchange["status"],
            "{name}, exchange {n}"
        );
        let recorded_sid = exchange["resp_headers"]["mcp-session-id"].as_str();
        let live_sid = response
            .headers()
            .get("mcp-session-id")
            .map(|v| v.to_str().unwrap().to_string());
        assert_eq!(
            recorded_sid.is_some(),
            live_sid.is_some(),
            "{name}, exchange {n}: session header"
        );
        if let (Some(recorded), Some(live)) = (recorded_sid, live_sid) {
            sessions.insert(recorded.to_string(), live);
        }

        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        let actual: Vec<Value> = if bytes.is_empty() {
            Vec::new()
        } else {
            vec![serde_json::from_slice(&bytes).unwrap()]
        };
        assert_eq!(Value::from(actual), exchange["s2c"], "{name}, exchange {n}");
    }
}

#[tokio::test]
async fn inspector_stdio_session_replays() {
    replay_stdio("inspector.stdio.jsonl").await;
}

#[tokio::test]
async fn claude_code_stdio_session_replays() {
    replay_stdio("claude.stdio.jsonl").await;
}

#[tokio::test]
async fn inspector_http_session_replays() {
    replay_http("inspector.http.jsonl").await;
}

#[tokio::test]
async fn claude_code_http_session_replays() {
    replay_http("claude.http.jsonl").await;
}
