//! Core type definitions for the MCP SDK.
//!
//! This module contains the fundamental types used throughout the SDK,
//! including content types (text, image, resource), error types, and
//! result types for tools, resources, and prompts.

pub mod content;
pub mod error;

pub use content::*;
pub use error::*;
