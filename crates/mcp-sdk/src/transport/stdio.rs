//! Stdio transport implementation for MCP.
//!
//! The stdio transport reads JSON-RPC messages from stdin and writes
//! responses to stdout. Each message is a single line of JSON terminated
//! by a newline character. This transport is used when MCP clients launch
//! the server as a subprocess (e.g., Claude Desktop, Cursor).
//!
//! Logging output should be directed to stderr, as stdout is reserved
//! for JSON-RPC protocol messages.

use async_trait::async_trait;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::sync::Mutex;
use tracing::{debug, trace};

use crate::protocol::types::{parse_jsonrpc_message, JsonRpcMessage};
use crate::transport::McpTransport;
use crate::types::error::TransportError;

/// MCP transport over standard input/output.
///
/// Messages are newline-delimited JSON-RPC objects. The server reads
/// from stdin and writes to stdout.
///
/// # Example
///
/// ```rust,no_run
/// use mcp_sdk::transport::stdio::StdioTransport;
/// use mcp_sdk::transport::McpTransport;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mut transport = StdioTransport::new();
/// // transport is now ready to receive and send messages
/// # Ok(())
/// # }
/// ```
pub struct StdioTransport {
    reader: Mutex<BufReader<io::Stdin>>,
    writer: Mutex<BufWriter<io::Stdout>>,
}

impl StdioTransport {
    /// Creates a new stdio transport using the process's stdin and stdout.
    #[must_use]
    pub fn new() -> Self {
        Self {
            reader: Mutex::new(BufReader::new(io::stdin())),
            writer: Mutex::new(BufWriter::new(io::stdout())),
        }
    }
}

impl Default for StdioTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for StdioTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StdioTransport").finish()
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn recv(&mut self) -> Result<Option<JsonRpcMessage>, TransportError> {
        loop {
            let mut line = String::new();
            let bytes_read = {
                let mut reader = self.reader.lock().await;
                reader
                    .read_line(&mut line)
                    .await
                    .map_err(TransportError::Io)?
            };

            if bytes_read == 0 {
                debug!("stdin EOF reached, transport closing");
                return Ok(None);
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                trace!("Skipping empty line from stdin");
                continue;
            }

            trace!(message_len = trimmed.len(), "Received message from stdin");

            let message = parse_jsonrpc_message(trimmed).map_err(|e| {
                TransportError::Deserialization(format!("Failed to parse JSON-RPC message: {e}"))
            })?;

            return Ok(Some(message));
        }
    }

    async fn send(&self, message: &JsonRpcMessage) -> Result<(), TransportError> {
        let json = serde_json::to_string(message).map_err(|e| {
            TransportError::Serialization(format!("Failed to serialize JSON-RPC message: {e}"))
        })?;

        trace!(message_len = json.len(), "Sending message to stdout");

        let mut writer = self.writer.lock().await;
        writer
            .write_all(json.as_bytes())
            .await
            .map_err(TransportError::Io)?;
        writer.write_all(b"\n").await.map_err(TransportError::Io)?;
        writer.flush().await.map_err(TransportError::Io)?;

        Ok(())
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        debug!("Closing stdio transport");
        let mut writer = self.writer.lock().await;
        writer.flush().await.map_err(TransportError::Io)?;
        Ok(())
    }
}

/// A stdio-like transport backed by in-memory buffers, for testing.
///
/// This transport reads from a `Cursor<Vec<u8>>` and writes to a
/// `Vec<u8>`, allowing tests to simulate the stdio protocol without
/// actual process I/O.
pub struct TestStdioTransport {
    reader: Mutex<io::BufReader<std::io::Cursor<Vec<u8>>>>,
    /// The output buffer. Access via `output()` after the server finishes.
    output: std::sync::Arc<Mutex<Vec<u8>>>,
}

impl TestStdioTransport {
    /// Creates a new test transport from input bytes.
    ///
    /// The input is consumed as if it were stdin. Output is collected
    /// in an internal buffer.
    pub fn new(input: Vec<u8>) -> Self {
        Self {
            reader: Mutex::new(io::BufReader::new(std::io::Cursor::new(input))),
            output: std::sync::Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Creates a new test transport from an input string.
    pub fn from_input(input: &str) -> Self {
        Self::new(input.as_bytes().to_vec())
    }

    /// Returns a clone of the output handle for reading after the server runs.
    pub fn output_handle(&self) -> std::sync::Arc<Mutex<Vec<u8>>> {
        std::sync::Arc::clone(&self.output)
    }
}

impl std::fmt::Debug for TestStdioTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestStdioTransport").finish()
    }
}

#[async_trait]
impl McpTransport for TestStdioTransport {
    async fn recv(&mut self) -> Result<Option<JsonRpcMessage>, TransportError> {
        loop {
            let mut line = String::new();
            let bytes_read = {
                let mut reader = self.reader.lock().await;
                reader
                    .read_line(&mut line)
                    .await
                    .map_err(TransportError::Io)?
            };

            if bytes_read == 0 {
                return Ok(None);
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let message = parse_jsonrpc_message(trimmed).map_err(|e| {
                TransportError::Deserialization(format!("Failed to parse JSON-RPC message: {e}"))
            })?;

            return Ok(Some(message));
        }
    }

    async fn send(&self, message: &JsonRpcMessage) -> Result<(), TransportError> {
        let json = serde_json::to_string(message).map_err(|e| {
            TransportError::Serialization(format!("Failed to serialize JSON-RPC message: {e}"))
        })?;

        let mut writer = self.output.lock().await;
        writer.extend_from_slice(json.as_bytes());
        writer.extend_from_slice(b"\n");

        Ok(())
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::types::RequestId;

    #[tokio::test]
    async fn test_send_and_recv() {
        let mut transport = TestStdioTransport::new(
            b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n".to_vec(),
        );

        let msg = transport.recv().await.expect("recv").expect("some message");
        match &msg {
            JsonRpcMessage::Request(req) => {
                assert_eq!(req.method, "ping");
                assert_eq!(req.id, RequestId::Number(1));
            }
            _ => panic!("Expected request"),
        }
    }

    #[tokio::test]
    async fn test_recv_eof() {
        let mut transport = TestStdioTransport::new(vec![]);
        let msg = transport.recv().await.expect("recv");
        assert!(msg.is_none());
    }

    #[tokio::test]
    async fn test_send_message() {
        let transport = TestStdioTransport::new(vec![]);
        let output_handle = transport.output_handle();

        let msg = JsonRpcMessage::response(RequestId::Number(1), serde_json::json!({}));
        transport.send(&msg).await.expect("send");

        let output = output_handle.lock().await;
        let output_str = String::from_utf8(output.clone()).expect("utf8");
        assert!(output_str.contains("\"jsonrpc\":\"2.0\""));
        assert!(output_str.ends_with('\n'));
    }

    #[tokio::test]
    async fn test_skip_empty_lines() {
        let mut transport = TestStdioTransport::new(
            b"\n\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n".to_vec(),
        );
        let msg = transport.recv().await.expect("recv").expect("some message");
        match msg {
            JsonRpcMessage::Request(req) => assert_eq!(req.method, "ping"),
            _ => panic!("Expected request"),
        }
    }

    #[tokio::test]
    async fn test_invalid_json() {
        let mut transport = TestStdioTransport::new(b"not valid json\n".to_vec());
        let result = transport.recv().await;
        assert!(result.is_err());
    }
}
