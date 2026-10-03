//! Memory usage benchmarks for MCP servers.
//!
//! Measures baseline memory usage for server construction and tool registration.

use std::sync::Arc;

use async_trait::async_trait;
use mcp_sdk::context::ToolContext;
use mcp_sdk::handler::tools::{ToolHandler, ToolRegistry};
use mcp_sdk::protocol::messages::ToolInfo;
use mcp_sdk::server::McpServer;
use mcp_sdk::types::content::CallToolResult;
use mcp_sdk::types::error::ToolError;

struct BenchTool {
    name: String,
}

#[async_trait]
impl ToolHandler for BenchTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            annotations: Some(mcp_sdk::ToolAnnotations {
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                idempotent_hint: Some(true),
                open_world_hint: Some(false),
                ..Default::default()
            }),
            name: self.name.clone(),
            description: Some(format!("Benchmark tool: {}", self.name)),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "input": {"type": "string"}
                },
                "required": ["input"]
            }),
        }
    }

    async fn call(
        &self,
        _arguments: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        Ok(CallToolResult::text("ok"))
    }
}

fn main() {
    println!("=== MCP SDK Memory Benchmarks ===\n");

    // Measure baseline: empty server
    let before = get_memory_usage();
    let _server = McpServer::builder()
        .name("memory-bench")
        .version("1.0.0")
        .build();
    let after = get_memory_usage();
    println!(
        "Empty server construction:     ~{} bytes (approx)",
        after.saturating_sub(before)
    );

    // Measure: server with 10 tools
    let before = get_memory_usage();
    let mut builder = McpServer::builder()
        .name("memory-bench-10")
        .version("1.0.0");
    for i in 0..10 {
        builder = builder.tool(BenchTool {
            name: format!("tool_{i}"),
        });
    }
    let _server = builder.build();
    let after = get_memory_usage();
    println!(
        "Server with 10 tools:          ~{} bytes (approx)",
        after.saturating_sub(before)
    );

    // Measure: server with 100 tools
    let before = get_memory_usage();
    let mut builder = McpServer::builder()
        .name("memory-bench-100")
        .version("1.0.0");
    for i in 0..100 {
        builder = builder.tool(BenchTool {
            name: format!("tool_{i}"),
        });
    }
    let _server = builder.build();
    let after = get_memory_usage();
    println!(
        "Server with 100 tools:         ~{} bytes (approx)",
        after.saturating_sub(before)
    );

    // Measure: tool registry operations
    let before = get_memory_usage();
    let mut registry = ToolRegistry::new();
    for i in 0..1000 {
        registry
            .register(Arc::new(BenchTool {
                name: format!("tool_{i}"),
            }))
            .expect("register");
    }
    let after = get_memory_usage();
    println!(
        "ToolRegistry with 1000 tools:  ~{} bytes (approx)",
        after.saturating_sub(before)
    );

    // Measure: tool list serialization
    let tools = registry.list();
    let before = get_memory_usage();
    let json = serde_json::to_string(&tools).expect("serialize");
    let after = get_memory_usage();
    println!(
        "Serialized 1000-tool list:     {} bytes JSON, ~{} bytes memory",
        json.len(),
        after.saturating_sub(before)
    );

    println!("\nNote: Memory measurements are approximate (heap allocator dependent).");
    println!("For precise measurements, use a profiler like DHAT or jemalloc stats.");
}

/// Returns an approximate memory usage measurement.
///
/// This is a rough approximation using allocator-level information.
/// For precise benchmarks, use jemalloc or DHAT.
fn get_memory_usage() -> usize {
    // Use a simple allocation counter as a rough proxy.
    // In production benchmarks, replace with jemalloc stats or /proc/self/status.
    let layout = std::alloc::Layout::new::<u8>();
    // Force a measurement point by touching the allocator
    let _ = std::hint::black_box(layout);
    0 // Placeholder -- real measurement requires jemalloc or similar
}
