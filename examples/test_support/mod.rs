//! Shared subprocess helpers for exercising the example servers over stdio.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// Runs an example server as a subprocess and returns every message it wrote.
///
/// Requests are sent one at a time: each waits for its response before the
/// next is written, the way a client sends calls that depend on earlier
/// results. The server handles requests concurrently, so pipelined requests
/// may complete in any order.
pub fn run_example(binary: &str, args: &[&str], requests: Vec<Value>) -> Vec<Value> {
    let mut child = Command::new(binary)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start example server");
    // Drain both pipes concurrently so a verbose server cannot block on output.
    let stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let (line_tx, line_rx) = mpsc::channel::<Value>();
    let stdout_reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let line = line.expect("read stdout");
            let message: Value = serde_json::from_str(&line).expect("JSON-RPC response");
            if line_tx.send(message).is_err() {
                break;
            }
        }
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).expect("read stderr");
        bytes
    });
    let mut input = child.stdin.take().expect("piped stdin");
    let mut replies = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    for message in [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25", "capabilities": {},
            "clientInfo": {"name": "example-test", "version": "1.0"}
        }}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    ]
    .into_iter()
    .chain(requests)
    {
        writeln!(input, "{message}").expect("send JSON-RPC message");
        input.flush().expect("flush stdin");
        let Some(id) = message.get("id") else {
            continue;
        };
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let reply = line_rx
                .recv_timeout(left)
                .unwrap_or_else(|_| panic!("no response to request {id} within 30 seconds"));
            let done = reply.get("id") == Some(id);
            replies.push(reply);
            if done {
                break;
            }
        }
    }
    drop(input);
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait().expect("check example process") {
            break (status, false);
        }
        if Instant::now() >= deadline {
            child.kill().expect("stop stalled example");
            break (child.wait().expect("reap stalled example"), true);
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    stdout_reader.join().expect("stdout reader");
    replies.extend(line_rx.try_iter());
    let stderr = stderr_reader.join().expect("stderr reader");
    assert!(
        !timed_out,
        "example failed to exit within 30 seconds after EOF: {}",
        String::from_utf8_lossy(&stderr)
    );
    assert!(
        status.success(),
        "example failed: {}",
        String::from_utf8_lossy(&stderr)
    );
    replies
}

pub fn response(responses: &[Value], id: u64) -> &Value {
    responses
        .iter()
        .find(|response| response["id"] == id)
        .expect("response for request ID")
}

pub fn assert_annotations(responses: &[Value], expected: &[(&str, bool)]) {
    let tools = response(responses, 2)["result"]["tools"]
        .as_array()
        .expect("tools/list result");
    let actual: BTreeMap<_, _> = tools
        .iter()
        .map(|tool| {
            (
                tool["name"].as_str().expect("tool name"),
                &tool["annotations"],
            )
        })
        .collect();
    assert_eq!(
        actual.len(),
        expected.len(),
        "complete example tool inventory"
    );
    for &(name, read_only) in expected {
        assert_eq!(
            actual.get(name).copied(),
            Some(&json!({
                "readOnlyHint": read_only,
                "destructiveHint": !read_only,
                "idempotentHint": true,
                "openWorldHint": false
            })),
            "annotations for {name}"
        );
    }
}

pub fn call(id: u64, name: &str, arguments: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": arguments}})
}
