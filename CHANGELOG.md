# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
