//! Arbitrary stdin never crashes or hangs the server, and everything it
//! writes to stdout is a valid JSON-RPC message (transports.mdx: "The server
//! MUST NOT write anything to its stdout that is not a valid MCP message").

#![no_main]

mod common;

use std::sync::OnceLock;
use std::time::Duration;

use libfuzzer_sys::fuzz_target;
use mcp_sdk::protocol::types::decode_jsonrpc_message;
use mcp_sdk::transport::stdio::TestStdioTransport;

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap())
}

fuzz_target!(|data: &[u8]| {
    let transport = TestStdioTransport::new(data.to_vec());
    let output = transport.output_handle();
    runtime().block_on(async {
        tokio::time::timeout(Duration::from_secs(5), common::server().run(transport))
            .await
            .expect("server must finish once stdin is exhausted")
            .expect("server run");
        let bytes = output.lock().await.clone();
        for line in String::from_utf8(bytes).expect("stdout is UTF-8").lines() {
            decode_jsonrpc_message(line).unwrap_or_else(|e| panic!("invalid stdout line {line:?}: {e}"));
        }
    });
});
