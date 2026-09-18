//! MCP protocol types and message handling.
//!
//! This module implements the JSON-RPC 2.0 message types used by the
//! Model Context Protocol, including request/response types, notification
//! types, capability negotiation, and MCP-specific message structures.

pub mod capabilities;
pub mod messages;
pub mod types;

pub use capabilities::*;
pub use messages::*;
pub use types::*;
