//! # mcp-sdk -- A high-performance Rust SDK for building MCP servers
//!
//! `mcp-sdk` provides a complete, ergonomic, and high-performance SDK for
//! building [Model Context Protocol (MCP)](https://modelcontextprotocol.io/)
//! servers in Rust. It brings the developer experience of modern Rust web
//! frameworks to the MCP ecosystem through a macro-based API.
//!
//! ## Quick Start
//!
//! Add `mcp-sdk` to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! mcp-sdk = "0.1"
//! tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
//! ```
//!
//! ## Building a Simple MCP Server
//!
//! ```rust,no_run
//! use mcp_sdk::prelude::*;
//! use std::sync::Arc;
//!
//! // Define a tool handler
//! struct AddTool;
//!
//! #[async_trait]
//! impl ToolHandler for AddTool {
//!     fn info(&self) -> ToolInfo {
//!         ToolInfo {
//!             name: "add".to_string(),
//!             description: Some("Add two numbers".to_string()),
//!             input_schema: JsonSchemaBuilder::new()
//!                 .property("a", JsonSchemaBuilder::number().description("First number"))
//!                 .property("b", JsonSchemaBuilder::number().description("Second number"))
//!                 .required("a")
//!                 .required("b")
//!                 .build(),
//!         }
//!     }
//!
//!     async fn call(
//!         &self,
//!         arguments: serde_json::Value,
//!         _context: &ToolContext,
//!     ) -> Result<CallToolResult, ToolError> {
//!         let a = arguments["a"].as_f64().unwrap_or(0.0);
//!         let b = arguments["b"].as_f64().unwrap_or(0.0);
//!         Ok(CallToolResult::text((a + b).to_string()))
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let server = McpServer::builder()
//!         .name("calculator")
//!         .version("1.0.0")
//!         .tool(AddTool)
//!         .build();
//!
//!     server.run(StdioTransport::new()).await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Architecture
//!
//! The SDK is organized into several layers:
//!
//! - **Protocol layer** ([`protocol`]): JSON-RPC 2.0 types, MCP message types,
//!   and capability negotiation.
//! - **Transport layer** ([`transport`]): Abstractions for stdio and HTTP
//!   transports.
//! - **Handler layer** ([`handler`]): Traits and registries for tools,
//!   resources, and prompts.
//! - **Server** ([`server`]): The `McpServer` builder that ties everything
//!   together.
//!
//! ## Concurrency
//!
//! The server processes tool calls, resource reads, and prompt gets
//! concurrently via `tokio::spawn`. This means multiple tool calls can
//! execute simultaneously, and `ping` responses are never blocked by
//! long-running tool operations.
//!
//! ## Cancellation
//!
//! The server supports the MCP `notifications/cancelled` notification.
//! When a client sends a cancellation, the corresponding tool call's
//! `ToolContext::is_cancelled()` returns `true`, allowing cooperative
//! cancellation of long-running operations.
//!
//! ## Feature Flags
//!
//! | Feature | Default | Description |
//! |---------|---------|-------------|
//! | `macros` | Yes | Enable `#[mcp_tool]`, `#[mcp_resource]`, `#[mcp_prompt]` proc macros |
//! | `stdio` | Yes | Enable stdio transport |
//! | `http` | No | Enable Streamable HTTP transport (Axum) |
//! | `sse` | No | Enable SSE streaming (implies `http`) |
//! | `full` | No | Enable all features |
//!
//! ## Minimum Supported Rust Version (MSRV)
//!
//! The MSRV for this crate is **1.75.0**, matching the requirement for
//! `async fn` in trait implementations via `async-trait`.

#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]

pub mod context;
pub mod handler;
pub mod prelude;
pub mod protocol;
pub mod schema;
pub mod server;
pub mod transport;
pub mod types;

// Re-export key types at the crate root for convenience
pub use context::ToolContext;
pub use protocol::capabilities::{ServerCapabilities, PROTOCOL_VERSION};
pub use protocol::messages::ToolInfo;
pub use server::McpServer;
pub use transport::McpTransport;
pub use types::content::{CallToolResult, Content, PromptMessage, ResourceContent};
pub use types::error::{McpError, PromptError, ResourceError, ToolError, TransportError};

// Re-export proc macros when the feature is enabled
#[cfg(feature = "macros")]
pub use mcp_macros::{mcp_prompt, mcp_resource, mcp_tool};
