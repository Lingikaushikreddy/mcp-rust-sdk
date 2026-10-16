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
- **MCP 2025-11-25 server** -- lifecycle, tools (with annotations), resources, prompts, ping and
  cancellation over JSON-RPC 2.0. Checked requirement by requirement in
  [docs/CONFORMANCE.md](docs/CONFORMANCE.md): it passes 62 of 72 checks in the official
  conformance suite's 2025-11-25 requirements, and every failing check is an optional feature
  listed under [Known Limitations](#known-limitations).
- **Two transports** -- stdio and Streamable HTTP (JSON responses, sessions, `Host`/`Origin`
  validation). Both are exercised by recorded sessions from MCP Inspector and Claude Code.
- **Concurrent requests** -- tool calls, resource reads and prompt gets run as tasks, so pings
  are answered and cancellations take effect while they run
- **Generated input schemas** -- `#[mcp_tool]` builds a tool's JSON Schema at compile time from its parameter types
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

### Serving over HTTP

Enable the `http` feature and add `axum = "0.7"`. Then serve the transport's
router with Axum and run the server on the same transport:

```rust,ignore
use mcp_sdk::prelude::*;
use mcp_sdk::transport::http::HttpTransport;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Binds 127.0.0.1:3000 and accepts only local Host/Origin headers by default.
    // Add your public host name with `.allowed_hosts(["mcp.example.com"])`.
    let transport = HttpTransport::builder().build();
    let listener = tokio::net::TcpListener::bind(transport.bind_addr()).await?;
    let router = transport.router();
    tokio::spawn(async move { axum::serve(listener, router).await });

    McpServer::builder().name("calculator").tool(AddTool).build().run(transport).await?;
    Ok(())
}
```

Tool handlers report failures by returning `Err(ToolError::...)`. The server sends
these to the client as a tool result with `isError: true`, so the model can read
the message and retry, as the specification asks. Only an unknown tool name
becomes a JSON-RPC error.

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
    conformance/   -- Fixture for the official MCP conformance suite, plus recorded client sessions
  benches/         -- Performance benchmarks
  fuzz/            -- cargo-fuzz targets for the JSON-RPC decoder and both transports
  docs/            -- CONFORMANCE.md
```

### Crate Features

| Feature | Default | Description |
|---------|---------|-------------|
| `macros` | Yes | `#[mcp_tool]`, `#[mcp_resource]`, `#[mcp_prompt]` proc macros |
| `stdio` | Yes | stdio transport (for Claude Desktop, Cursor, etc.) |
| `http` | No | Streamable HTTP transport (Axum-based) |
| `sse` | No | SSE event types (`SseEvent`, `SseManager`); implies `http`. Not used by the HTTP transport, which answers with JSON |
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

- Server-side only -- there is no MCP client.
- The server cannot send messages of its own: no log or progress notifications,
  no `listChanged` notifications, no sampling or elicitation requests. Over HTTP,
  every request is answered with one JSON response and GET returns `405`
  (allowed by the specification); there are no SSE streams or resumability.
- Not implemented: completions, resource subscriptions, pagination, audio and
  resource-link content, `structuredContent`/`outputSchema`, tool `title`/`icons`,
  tasks, and OAuth authorization.
- HTTP `session_timeout` and `max_sessions` are accepted but not enforced yet.
- `resources/read` for an unknown URI returns `-32602`. The 2025-11-25 text
  suggests `-32002`, but the next revision standardizes on `-32602`
  ([details](docs/CONFORMANCE.md#resources-and-prompts)).

[docs/CONFORMANCE.md](docs/CONFORMANCE.md) lists every requirement checked, with
the spec line and the test or recording that backs it.

## Supported MCP Protocol Versions

- **2025-11-25** -- primary target. A client asking for any other version except
  2024-11-05 is offered 2025-11-25.
- **2024-11-05** -- echoed back during negotiation so older stdio clients can connect.
  The responses use the 2025-11-25 shapes, and that version's HTTP+SSE transport
  is not offered.
- **2026-07-28** (the newest revision) -- not supported. Its stateless
  `server/discover` probe is rejected, and clients that implement the revision's
  downgrade path, such as Claude Code, fall back to `initialize`.

## How this differs from rmcp

[`rmcp`](https://github.com/modelcontextprotocol/rust-sdk) is the official Rust SDK,
maintained under the Model Context Protocol organization. It implements both the
client and the server side, tracks the newest protocol revision, and covers most
of the features listed above as missing here. If you need an MCP SDK in Rust for
production, use rmcp. This project is a smaller, server-only SDK. It offers a
builder plus hand-written handler traits
and three proc macros, and over HTTP it answers each request with plain JSON
instead of an SSE stream. Its value is mostly its record:
[docs/CONFORMANCE.md](docs/CONFORMANCE.md) checks it line by line against the
2025-11-25 specification, the official conformance suite, rmcp and the TypeScript
SDK. That record shows where it conforms, where it does not, and which features it
lacks.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.
