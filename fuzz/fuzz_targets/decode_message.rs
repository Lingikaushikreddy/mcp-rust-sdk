//! The JSON-RPC decoder never panics, and whatever it accepts re-encodes to
//! JSON that decodes to the same message.

#![no_main]

use libfuzzer_sys::fuzz_target;
use mcp_sdk::protocol::types::decode_jsonrpc_message;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else { return };
    if let Ok(message) = decode_jsonrpc_message(text) {
        let encoded = serde_json::to_string(&message).expect("encode");
        let again = decode_jsonrpc_message(&encoded).expect("re-decode");
        assert_eq!(message, again);
    }
});
