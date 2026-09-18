//! Stdio transport tests.

use mcp_sdk::protocol::types::{JsonRpcMessage, RequestId};
use mcp_sdk::transport::stdio::TestStdioTransport;
use mcp_sdk::transport::McpTransport;

#[tokio::test]
async fn test_recv_valid_request() {
    let mut transport =
        TestStdioTransport::new(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n".to_vec());

    let msg = transport.recv().await.expect("recv").expect("some");
    match msg {
        JsonRpcMessage::Request(req) => {
            assert_eq!(req.method, "ping");
            assert_eq!(req.id, RequestId::Number(1));
        }
        _ => panic!("Expected request"),
    }
}

#[tokio::test]
async fn test_recv_notification() {
    let mut transport = TestStdioTransport::new(
        b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n".to_vec(),
    );

    let msg = transport.recv().await.expect("recv").expect("some");
    match msg {
        JsonRpcMessage::Notification(notif) => {
            assert_eq!(notif.method, "notifications/initialized");
        }
        _ => panic!("Expected notification"),
    }
}

#[tokio::test]
async fn test_recv_eof() {
    let mut transport = TestStdioTransport::new(vec![]);
    let msg = transport.recv().await.expect("recv");
    assert!(msg.is_none());
}

#[tokio::test]
async fn test_recv_invalid_json() {
    let mut transport = TestStdioTransport::new(b"this is not json\n".to_vec());
    let result = transport.recv().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_recv_missing_jsonrpc_version() {
    let mut transport = TestStdioTransport::new(b"{\"id\":1,\"method\":\"test\"}\n".to_vec());
    let result = transport.recv().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_send_response() {
    let transport = TestStdioTransport::new(vec![]);
    let output_handle = transport.output_handle();

    let msg = JsonRpcMessage::response(RequestId::Number(1), serde_json::json!({"tools": []}));
    transport.send(&msg).await.expect("send");

    let output = output_handle.lock().await;
    let output_str = String::from_utf8(output.clone()).expect("utf8");
    assert!(output_str.contains("\"jsonrpc\":\"2.0\""));
    assert!(output_str.contains("\"id\":1"));
    assert!(output_str.contains("\"tools\":[]"));
    assert!(output_str.ends_with('\n'));
}

#[tokio::test]
async fn test_send_error_response() {
    let transport = TestStdioTransport::new(vec![]);
    let output_handle = transport.output_handle();

    let msg = JsonRpcMessage::error_response(
        RequestId::Number(42),
        mcp_sdk::types::error::JsonRpcError::method_not_found(Some("test".to_string())),
    );
    transport.send(&msg).await.expect("send");

    let output = output_handle.lock().await;
    let output_str = String::from_utf8(output.clone()).expect("utf8");
    assert!(output_str.contains("-32601"));
}

#[tokio::test]
async fn test_multiple_messages() {
    let input = concat!(
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n",
        "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\n"
    );
    let mut transport = TestStdioTransport::from_input(input);

    // First message: initialize request
    let msg1 = transport.recv().await.expect("recv").expect("some");
    assert!(matches!(msg1, JsonRpcMessage::Request(_)));

    // Second message: initialized notification
    let msg2 = transport.recv().await.expect("recv").expect("some");
    assert!(matches!(msg2, JsonRpcMessage::Notification(_)));

    // Third message: ping request
    let msg3 = transport.recv().await.expect("recv").expect("some");
    assert!(matches!(msg3, JsonRpcMessage::Request(_)));

    // EOF
    let msg4 = transport.recv().await.expect("recv");
    assert!(msg4.is_none());
}

#[tokio::test]
async fn test_close() {
    let mut transport = TestStdioTransport::new(vec![]);
    let result = transport.close().await;
    assert!(result.is_ok());
}
