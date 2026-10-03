# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Optional `ToolInfo.annotations` and `ToolAnnotations` with a title and the four MCP behavior hints.
- `#[mcp_tool]` annotation arguments, including explicit boolean values; the existing bare `destructive` flag now emits `destructiveHint: true`.
- Explicit behavior annotations for every calculator, filesystem, and database example tool, with an annotation guide in the README.

### Fixed

- Keep Clippy compatible with newer Rust versions by allowing only the redundant `must_use` attributes generated on async handler and transport traits.

### Migration

- Existing Rust `ToolInfo` struct literals must add `annotations: None` or `Some(ToolAnnotations { ... })`. JSON metadata without annotations remains supported, and absent fields stay omitted.
- The tool macro now rejects unknown or duplicate options and malformed annotation values instead of silently ignoring them.

## [0.1.0] - 2026-03-13

### Added

- Core MCP protocol implementation (protocol version 2025-11-25, JSON-RPC 2.0).
- `#[mcp_tool]`, `#[mcp_resource]`, and `#[mcp_prompt]` proc macros for ergonomic handler definitions.
- Stdio transport for integration with Claude Desktop, Cursor, and other local clients.
- Streamable HTTP transport (Axum-based) for remote server deployments.
- Tool, resource, and prompt handler traits with async support.
- `McpServer` builder API for composing servers from handlers.
- Type-safe JSON Schema generation via `JsonSchemaBuilder`.
- Session management for the HTTP transport with configurable timeouts.
- Calculator, filesystem, and database example servers.
- Performance benchmarks (throughput, memory).
- Structured logging via the `tracing` crate.
- Dual license under MIT and Apache-2.0.

[Unreleased]: https://github.com/Lingikaushikreddy/mcp-rust-sdk/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Lingikaushikreddy/mcp-rust-sdk/releases/tag/v0.1.0
