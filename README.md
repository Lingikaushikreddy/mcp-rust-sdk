# mcp-rust-sdk

A Rust SDK for building [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) servers.

The SDK provides protocol types, handler traits, transports, and a server builder.
The calculator, filesystem, and database servers in `examples/` demonstrate those
APIs; applications define and register their own tools. Echo handlers appear in
documentation, tests, and benchmarks as fixtures.

[![CI](https://github.com/Lingikaushikreddy/mcp-rust-sdk/actions/workflows/ci.yml/badge.svg)](https://github.com/Lingikaushikreddy/mcp-rust-sdk/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![M8ven Score](https://m8ven.ai/badge/mcp/lingikaushikreddy/mcp-rust-sdk)](https://m8ven.ai/mcp/lingikaushikreddy/mcp-rust-sdk?s=readme)

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
            annotations: Some(ToolAnnotations {
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                idempotent_hint: Some(true),
                open_world_hint: Some(false),
                ..Default::default()
            }),
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

#[mcp_tool(
    description = "Add two numbers",
    title = "Add numbers",
    read_only_hint = true,
    destructive_hint = false,
    idempotent_hint = true,
    open_world_hint = false
)]
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

### Tool Annotations

`ToolInfo.annotations` adds optional metadata to `tools/list` responses. Manual
handlers return `Some(ToolAnnotations { ... })` as shown above; the macro accepts
a string literal for `title` and boolean literals for the four hints. Rust field
and macro argument names use snake_case; JSON hint names use camelCase.

The [MCP annotation definitions](https://modelcontextprotocol.io/specification/2025-11-25/schema#toolannotations)
describe these fields:

| Rust field | JSON field | Meaning | MCP default when omitted |
|------------|------------|---------|--------------------------|
| `title` | `title` | Display title | No annotation title |
| `read_only_hint` | `readOnlyHint` | Does not modify its environment | `false` |
| `destructive_hint` | `destructiveHint` | May delete or overwrite state | `true` |
| `idempotent_hint` | `idempotentHint` | Repeating the same arguments has no additional effect on the environment | `false` |
| `open_world_hint` | `openWorldHint` | May interact with an open domain of external entities | `true` |

`destructiveHint` and `idempotentHint` are meaningful when `readOnlyHint` is false.
Idempotence concerns effects on the environment; repeated calls may return
different messages. For example, `db_delete` is idempotent even when a second
call reports that the key is absent. `db_set` is destructive because it can
overwrite an existing value.

Annotations are hints supplied by the server. Clients should not base tool use
decisions on annotations from untrusted servers. The SDK serializes them without
enforcing read access, restricting writes, or adding authorization checks.

The SDK preserves optionality: `ToolAnnotations::default()` leaves every field
as `None`, and `annotations: None` omits the entire object. A hint set to
`Some(false)` is included in JSON. The SDK does not fill in the MCP defaults or
infer hints from tool names or handler behavior. Macros without annotation
arguments also leave the object absent. The existing bare `destructive` macro
argument is supported as shorthand for `destructive_hint = true`.
Use either `destructive` or `destructive_hint`; combining them is rejected.
Unknown options, duplicate options, and hint values other than boolean literals
produce compile errors so misspelled metadata cannot be silently discarded.

The example tools declare all four boolean hints. Calculator tools and filesystem
reads declare a closed domain; filesystem paths are checked against the configured
base directory. Database tools operate on the server's in-memory store.

#### Migrating Manual Handlers

Adding `annotations` is a source-breaking change for existing Rust `ToolInfo`
struct literals: they must add the field. To keep metadata absent, use:

```rust
use mcp_sdk::prelude::*;

let info = ToolInfo {
    name: "custom_tool".to_string(),
    description: None,
    input_schema: JsonSchemaBuilder::new().build(),
    annotations: None,
};
```

Use `Some(ToolAnnotations { ... })` to describe a tool's behavior. Existing JSON
metadata without annotations remains valid, and `None` preserves its omission
on the wire.

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

- **2025-11-25** -- primary target
- **2024-11-05** -- accepted during version negotiation

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.
