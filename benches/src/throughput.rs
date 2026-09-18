//! Throughput benchmarks for MCP tool calls.
//!
//! Measures requests per second for tool listing and calling.

use std::sync::Arc;

use async_trait::async_trait;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mcp_sdk::context::ToolContext;
use mcp_sdk::handler::tools::{ToolHandler, ToolRegistry};
use mcp_sdk::protocol::messages::ToolInfo;
use mcp_sdk::schema::JsonSchemaBuilder;
use mcp_sdk::types::content::CallToolResult;
use mcp_sdk::types::error::ToolError;

struct EchoTool;

#[async_trait]
impl ToolHandler for EchoTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "echo".to_string(),
            description: Some("Echo tool for benchmarking".to_string()),
            input_schema: JsonSchemaBuilder::new()
                .property("message", JsonSchemaBuilder::string())
                .required("message")
                .build(),
        }
    }

    async fn call(
        &self,
        arguments: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        let msg = arguments
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        Ok(CallToolResult::text(msg))
    }
}

fn bench_tool_list(c: &mut Criterion) {
    let mut registry = ToolRegistry::new();
    for i in 0..10 {
        struct NumTool(usize);
        #[async_trait]
        impl ToolHandler for NumTool {
            fn info(&self) -> ToolInfo {
                ToolInfo {
                    name: format!("tool_{}", self.0),
                    description: Some(format!("Tool number {}", self.0)),
                    input_schema: serde_json::json!({"type": "object"}),
                }
            }
            async fn call(
                &self,
                _args: serde_json::Value,
                _ctx: &ToolContext,
            ) -> Result<CallToolResult, ToolError> {
                Ok(CallToolResult::text("ok"))
            }
        }
        registry.register(Arc::new(NumTool(i))).expect("register");
    }

    c.bench_function("tool_list_10_tools", |b| {
        b.iter(|| {
            let tools = registry.list();
            black_box(tools);
        });
    });
}

fn bench_tool_call(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(EchoTool)).expect("register");

    c.bench_function("tool_call_echo", |b| {
        b.to_async(&rt).iter(|| async {
            let ctx = ToolContext::default();
            let result = registry
                .call("echo", serde_json::json!({"message": "benchmark"}), &ctx)
                .await
                .expect("call");
            black_box(result);
        });
    });
}

fn bench_json_parse(c: &mut Criterion) {
    let request_json = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"echo","arguments":{"message":"hello"}}}"#;

    c.bench_function("json_parse_tool_call_request", |b| {
        b.iter(|| {
            let msg = mcp_sdk::protocol::types::parse_jsonrpc_message(black_box(request_json))
                .expect("parse");
            black_box(msg);
        });
    });
}

fn bench_json_serialize(c: &mut Criterion) {
    let result = CallToolResult::text("benchmark result");

    c.bench_function("json_serialize_tool_result", |b| {
        b.iter(|| {
            let json = serde_json::to_string(black_box(&result)).expect("serialize");
            black_box(json);
        });
    });
}

criterion_group!(
    benches,
    bench_tool_list,
    bench_tool_call,
    bench_json_parse,
    bench_json_serialize,
);
criterion_main!(benches);
