//! Calculator MCP Server
//!
//! A simple MCP server that provides basic arithmetic operations as tools.
//! Demonstrates how to build an MCP server using the builder pattern with
//! manually implemented tool handlers.

use async_trait::async_trait;
use mcp_sdk::context::ToolContext;
use mcp_sdk::handler::tools::ToolHandler;
use mcp_sdk::protocol::messages::ToolInfo;
use mcp_sdk::schema::JsonSchemaBuilder;
use mcp_sdk::server::McpServer;
use mcp_sdk::transport::stdio::StdioTransport;
use mcp_sdk::types::content::CallToolResult;
use mcp_sdk::types::error::ToolError;
use tracing_subscriber::EnvFilter;

// ---------------------------------------------------------------------------
// Tool Handlers
// ---------------------------------------------------------------------------

/// Tool that adds two numbers.
struct AddTool;

#[async_trait]
impl ToolHandler for AddTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "add".to_string(),
            description: Some("Add two numbers together".to_string()),
            input_schema: JsonSchemaBuilder::new()
                .property("a", JsonSchemaBuilder::number().description("First number"))
                .property(
                    "b",
                    JsonSchemaBuilder::number().description("Second number"),
                )
                .required("a")
                .required("b")
                .build(),
        }
    }

    async fn call(
        &self,
        arguments: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        let a = arguments
            .get("a")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| ToolError::InvalidParams("Missing or invalid 'a'".to_string()))?;
        let b = arguments
            .get("b")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| ToolError::InvalidParams("Missing or invalid 'b'".to_string()))?;

        let result = a + b;
        Ok(CallToolResult::text(format_number(result)))
    }
}

/// Tool that subtracts two numbers.
struct SubtractTool;

#[async_trait]
impl ToolHandler for SubtractTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "subtract".to_string(),
            description: Some("Subtract the second number from the first".to_string()),
            input_schema: JsonSchemaBuilder::new()
                .property(
                    "a",
                    JsonSchemaBuilder::number().description("Number to subtract from"),
                )
                .property(
                    "b",
                    JsonSchemaBuilder::number().description("Number to subtract"),
                )
                .required("a")
                .required("b")
                .build(),
        }
    }

    async fn call(
        &self,
        arguments: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        let a = arguments
            .get("a")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| ToolError::InvalidParams("Missing or invalid 'a'".to_string()))?;
        let b = arguments
            .get("b")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| ToolError::InvalidParams("Missing or invalid 'b'".to_string()))?;

        let result = a - b;
        Ok(CallToolResult::text(format_number(result)))
    }
}

/// Tool that multiplies two numbers.
struct MultiplyTool;

#[async_trait]
impl ToolHandler for MultiplyTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "multiply".to_string(),
            description: Some("Multiply two numbers together".to_string()),
            input_schema: JsonSchemaBuilder::new()
                .property("a", JsonSchemaBuilder::number().description("First factor"))
                .property(
                    "b",
                    JsonSchemaBuilder::number().description("Second factor"),
                )
                .required("a")
                .required("b")
                .build(),
        }
    }

    async fn call(
        &self,
        arguments: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        let a = arguments
            .get("a")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| ToolError::InvalidParams("Missing or invalid 'a'".to_string()))?;
        let b = arguments
            .get("b")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| ToolError::InvalidParams("Missing or invalid 'b'".to_string()))?;

        let result = a * b;
        Ok(CallToolResult::text(format_number(result)))
    }
}

/// Tool that divides two numbers.
struct DivideTool;

#[async_trait]
impl ToolHandler for DivideTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "divide".to_string(),
            description: Some("Divide the first number by the second".to_string()),
            input_schema: JsonSchemaBuilder::new()
                .property(
                    "a",
                    JsonSchemaBuilder::number().description("Dividend (number to divide)"),
                )
                .property(
                    "b",
                    JsonSchemaBuilder::number().description("Divisor (number to divide by)"),
                )
                .required("a")
                .required("b")
                .build(),
        }
    }

    async fn call(
        &self,
        arguments: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        let a = arguments
            .get("a")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| ToolError::InvalidParams("Missing or invalid 'a'".to_string()))?;
        let b = arguments
            .get("b")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| ToolError::InvalidParams("Missing or invalid 'b'".to_string()))?;

        if b == 0.0 {
            return Err(ToolError::ExecutionError("Division by zero".to_string()));
        }

        let result = a / b;
        Ok(CallToolResult::text(format_number(result)))
    }
}

/// Formats a number, showing integers without decimal points.
fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < i64::MAX as f64 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging to stderr (stdout is reserved for JSON-RPC)
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let server = McpServer::builder()
        .name("calculator")
        .version("1.0.0")
        .instructions("A calculator server that performs basic arithmetic operations.")
        .tool(AddTool)
        .tool(SubtractTool)
        .tool(MultiplyTool)
        .tool(DivideTool)
        .build();

    tracing::info!("Calculator MCP server starting on stdio");
    server.run(StdioTransport::new()).await?;

    Ok(())
}
