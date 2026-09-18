# mcp-rust-sdk

A Rust SDK for building [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) servers.

[![CI](https://github.com/Lingikaushikreddy/mcp-rust-sdk/actions/workflows/ci.yml/badge.svg)](https://github.com/Lingikaushikreddy/mcp-rust-sdk/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

> **Status:** early development (0.1.0). Not published to crates.io yet -- the
> `mcp-sdk` name there belongs to an unrelated project, so depend on this repo via git.

## Features

- **Ergonomic API** -- Builder pattern and `#[mcp_tool]`, `#[mcp_resource]`, `#[mcp_prompt]` proc macros
- **MCP protocol** -- targets protocol version 2025-11-25 over JSON-RPC 2.0 (tools, resources, prompts)
- **Multiple transports** -- stdio (for Claude Desktop/Cursor) and Streamable HTTP POST (for remote servers)
- **Type-safe** -- Compile-time JSON Schema generation from Rust types
- **Observable** -- structured logging via `tracing`, typed errors, and no `unsafe` code

## Quick Start

Add the SDK to your `Cargo.toml` as a git dependency:

```toml
[dependencies]
mcp-sdk = { git = "https://github.com/Lingikaushikreddy/mcp-rust-sdk" }
tokio = { version = "1", features = ["full"] }
```

### Building a Calculator Server

```rust
use mcp_sdk::prelude::*;
use std::sync::Arc;

struct AddTool;

#[async_trait]
impl ToolHandler for AddTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "add".to_string(),
            description: Some("Add two numbers".to_string()),
            input_schema: JsonSchemaBuilder::new()
                .property("a", JsonSchemaBuilder::number().description("First number"))
                .property("b", JsonSchemaBuilder::number().description("Second number"))
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
        let a = arguments["a"].as_f64().unwrap_or(0.0);
        let b = arguments["b"].as_f64().unwrap_or(0.0);
        Ok(CallToolResult::text((a + b).to_string()))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = McpServer::builder()
        .name("calculator")
        .version("1.0.0")
        .tool(AddTool)
        .build();

    server.run(StdioTransport::new()).await?;
    Ok(())
}
```

### Using Proc Macros

```rust
use mcp_sdk::prelude::*;

#[mcp_tool(description = "Add two numbers")]
async fn add(a: f64, b: f64) -> Result<CallToolResult, ToolError> {
    Ok(CallToolResult::text((a + b).to_string()))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = McpServer::builder()
        .name("calculator")
        .version("1.0.0")
        .tool(add())  // Use the generated handler
        .build();

    server.run(StdioTransport::new()).await?;
    Ok(())
}
```

## Architecture

```
mcp-rust-sdk/
  crates/
    mcp-sdk/       -- Core SDK crate (transport, protocol, handlers, server)
    mcp-macros/    -- Proc macro crate (#[mcp_tool], #[mcp_resource], #[mcp_prompt])
  examples/
    calculator/    -- Basic arithmetic MCP server
    filesystem/    -- File system operations MCP server
    database/      -- In-memory key-value database MCP server
  benches/         -- Performance benchmarks
```

### Crate Features

| Feature | Default | Description |
|---------|---------|-------------|
| `macros` | Yes | `#[mcp_tool]`, `#[mcp_resource]`, `#[mcp_prompt]` proc macros |
| `stdio` | Yes | stdio transport (for Claude Desktop, Cursor, etc.) |
| `http` | No | Streamable HTTP transport (Axum-based) |
| `sse` | No | Server-Sent Events streaming (implies `http`; GET stream not implemented yet) |
| `full` | No | All features enabled |

## Examples

Run the example servers:

```bash
# Calculator server (stdio)
cargo run -p mcp-example-calculator

# Filesystem server (stdio)
cargo run -p mcp-example-filesystem

# Database server (stdio)
cargo run -p mcp-example-database
```

### Claude Desktop Configuration

Add to your Claude Desktop config (`~/Library/Application Support/Claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "calculator": {
      "command": "/path/to/target/release/mcp-example-calculator"
    }
  }
}
```

## Benchmarks

Throughput and memory benchmarks live in `benches/`. No published numbers yet;
run them locally:

```bash
cargo bench --bench throughput
cargo bench --bench memory
```

## Development

```bash
# Build everything
cargo build --workspace --all-features

# Run all tests
cargo test --workspace --all-features

# Lint
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Format
cargo fmt --all
```

## Known Limitations

- The HTTP transport handles POST requests; the GET/SSE stream returns `405` for now.
- Requests are processed sequentially per connection.
- Server-side only -- there is no MCP client yet.
- Pagination, resource subscriptions, sampling and cancellation are not implemented.

See [AUDIT_REPORT.md](AUDIT_REPORT.md) for the full list of open items.

## Supported MCP Protocol Versions

- **2025-11-25** -- Full support (primary target)
- **2024-11-05** -- accepted during version negotiation

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.
