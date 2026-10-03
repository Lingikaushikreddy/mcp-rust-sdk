//! Procedural macros for the `mcp-sdk` crate.
//!
//! This crate provides three attribute macros for defining MCP handlers:
//!
//! - [`mcp_tool`] -- Converts an async function into an MCP tool handler.
//! - [`mcp_resource`] -- Converts an async function into an MCP resource handler.
//! - [`mcp_prompt`] -- Converts an async function into an MCP prompt handler.
//!
//! These macros generate the boilerplate needed to register handlers with
//! the MCP server, including JSON Schema generation and trait implementations.

extern crate proc_macro;

mod prompt;
mod resource;
mod schema;
mod tool;
mod utils;

use proc_macro::TokenStream;

/// Converts an async function into an MCP tool handler.
///
/// The macro generates a struct that implements `ToolHandler`, builds the
/// JSON Schema from the function's parameter types, and creates a factory
/// function that returns the handler instance.
///
/// # Attributes
///
/// - `name = "..."` -- Override the tool name (defaults to the function name).
/// - `description = "..."` -- Tool description (defaults to doc comment).
/// - `title = "..."` -- Human-readable title in tool annotations.
/// - `read_only_hint = true/false` -- Whether the tool does not modify its environment.
/// - `destructive_hint = true/false` -- Whether the tool may perform destructive updates.
/// - `idempotent_hint = true/false` -- Whether repeated calls with the same arguments
///   have no additional effect.
/// - `open_world_hint = true/false` -- Whether the tool may interact with external entities.
/// - `destructive` -- Legacy shorthand for `destructive_hint = true`.
///
/// Omitted hints remain unspecified. Tools with no annotation attributes omit
/// annotations entirely. Annotation hints describe tool behavior and do not
/// enforce permissions. Unknown options, duplicates (including the legacy alias),
/// and values of the wrong type are rejected at compile time.
///
/// # Example
///
/// ```rust,ignore
/// use mcp_sdk::prelude::*;
///
/// #[mcp_tool(
///     description = "Add two numbers",
///     read_only_hint = true,
///     destructive_hint = false,
///     idempotent_hint = true,
///     open_world_hint = false,
/// )]
/// async fn add(a: f64, b: f64) -> Result<CallToolResult, ToolError> {
///     Ok(CallToolResult::text((a + b).to_string()))
/// }
///
/// // Use the tool:
/// let server = McpServer::builder()
///     .tool(add())
///     .build();
/// ```
#[proc_macro_attribute]
pub fn mcp_tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    match tool::expand(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Converts an async function into an MCP resource handler.
///
/// The macro generates a struct that implements `ResourceHandler` with
/// URI template matching support.
///
/// # Attributes
///
/// - `uri = "..."` -- The resource URI or URI template (required).
/// - `name = "..."` -- Override the resource name (defaults to function name).
/// - `description = "..."` -- Resource description (defaults to doc comment).
/// - `mime_type = "..."` -- The MIME type of the resource content.
///
/// # Example
///
/// ```rust,ignore
/// use mcp_sdk::prelude::*;
///
/// #[mcp_resource(
///     uri = "file:///{path}",
///     description = "Read a file",
///     mime_type = "text/plain"
/// )]
/// async fn read_file(path: String) -> Result<ResourceContent, ResourceError> {
///     let content = tokio::fs::read_to_string(&path).await
///         .map_err(|e| ResourceError::Io(e))?;
///     Ok(ResourceContent::text(format!("file:///{path}"), content))
/// }
/// ```
#[proc_macro_attribute]
pub fn mcp_resource(attr: TokenStream, item: TokenStream) -> TokenStream {
    match resource::expand(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Converts an async function into an MCP prompt handler.
///
/// The macro generates a struct that implements `PromptHandler` with
/// argument extraction from a `HashMap<String, String>`.
///
/// # Attributes
///
/// - `name = "..."` -- Override the prompt name (defaults to function name).
/// - `description = "..."` -- Prompt description (defaults to doc comment).
///
/// # Example
///
/// ```rust,ignore
/// use mcp_sdk::prelude::*;
///
/// #[mcp_prompt(description = "Generate a code review")]
/// async fn code_review(
///     code: String,
///     language: Option<String>,
/// ) -> Result<Vec<PromptMessage>, PromptError> {
///     Ok(vec![
///         PromptMessage::user(format!(
///             "Review this {} code:\n```\n{}\n```",
///             language.unwrap_or_else(|| "unknown".to_string()),
///             code
///         )),
///     ])
/// }
/// ```
#[proc_macro_attribute]
pub fn mcp_prompt(attr: TokenStream, item: TokenStream) -> TokenStream {
    match prompt::expand(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
