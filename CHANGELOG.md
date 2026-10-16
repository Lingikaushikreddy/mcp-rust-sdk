# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Optional `ToolInfo.annotations` and `ToolAnnotations` with a title and the four MCP behavior hints.
- `#[mcp_tool]` annotation arguments, including explicit boolean values; the existing bare `destructive` flag now emits `destructiveHint: true`.
- Explicit behavior annotations for every calculator, filesystem, and database example tool, with an annotation guide in the README.
- `docs/CONFORMANCE.md`: a requirement-by-requirement record against MCP 2025-11-25, with spec citations and evidence.
- `examples/conformance`: a fixture server for the official MCP conformance suite, plus replay tests of recorded MCP Inspector and Claude Code sessions.
- `fuzz/`: cargo-fuzz targets for the JSON-RPC decoder, the stdio session loop and HTTP POST handling.
- `HttpTransportBuilder::allowed_hosts` and `allow_any_host`; `SUPPORTED_PROTOCOL_VERSIONS`; `decode_jsonrpc_message`; `TransportError::InvalidMessage`; `JsonRpcMessage::error_response_without_id`; `ToolError::message`.

### Fixed

- Keep Clippy compatible with newer Rust versions by allowing only the redundant `must_use` attributes generated on async handler and transport traits.
- Compare empty registry results directly in tests so failures display unexpected contents and satisfy newer Clippy diagnostics.
- Tool handler errors are returned as `CallToolResult` with `isError: true` instead of JSON-RPC errors; unknown tools remain `-32602`.
- Requests run concurrently: ping is answered while tools run, and `notifications/cancelled` stops a request and suppresses its response. Previously each request was awaited before the next message was read.
- Invalid JSON is answered with `-32700` and invalid JSON-RPC messages with `-32600`, both without an id (was `-32700` with id `0`). A panicking handler is answered with its own request id.
- `ping` is answered before `initialize`.
- HTTP: `Host`/`Origin` validation (403), default bind `127.0.0.1`, `MCP-Protocol-Version` validation (400), the session id is returned on initialize and required afterwards, 202 responses have no body, error bodies carry no id, and responses can no longer be delivered to a different session that reused the same request id.

### Migration

- Existing Rust `ToolInfo` struct literals must add `annotations: None` or `Some(ToolAnnotations { ... })`. JSON metadata without annotations remains supported, and absent fields stay omitted.
- The tool macro now rejects unknown or duplicate options and malformed annotation values instead of silently ignoring them.
- `JsonRpcErrorResponse.id` is now `Option<RequestId>`.
- Clients that relied on tool failures arriving as JSON-RPC errors must read `result.isError` instead.
- HTTP servers reached through a public host name must add it with `allowed_hosts`; HTTP clients must send the `Mcp-Session-Id` returned by `initialize` (or the server must use `enable_sessions(false)`).
- Custom `McpTransport` implementations must make `recv` cancel-safe.
- Requests sent without waiting for earlier responses can now complete in any order. Clients whose calls depend on earlier results must wait for those responses first, as the example tests now do.

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
