//! Handler traits and registries for MCP tools, resources, and prompts.
//!
//! This module defines the core traits that user code implements to handle
//! MCP tool calls, resource reads, and prompt generation. It also provides
//! registries that collect handlers and dispatch incoming requests.

pub mod prompts;
pub mod resources;
pub mod tools;

pub use prompts::*;
pub use resources::*;
pub use tools::*;
