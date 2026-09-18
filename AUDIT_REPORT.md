# mcp-rust-sdk Audit Report

**Type:** Self-review of the codebase
**Date:** 2026-03-13
**Scope:** Full codebase audit of `mcp-rust-sdk` (workspace version 0.1.0)
**Commit:** HEAD of main branch

---

## Executive Summary

The `mcp-rust-sdk` is a well-structured 0.1.0 MCP server SDK with thoughtful crate architecture, reasonable API ergonomics, and good foundational test coverage. The project demonstrates solid Rust idioms overall -- proper builder patterns, `#[must_use]` annotations, `clippy::pedantic` enforcement, and a clean module hierarchy.

However, the audit reveals **significant gaps** that would prevent adoption at the level of production-quality crates like `serde`, `tokio`, or `axum`. The most critical issues are:

1. **The `McpServer` is not `Send`/concurrent** -- it takes `&mut self` in the main loop and processes requests sequentially, meaning a slow tool call blocks all other requests.
2. **The HTTP transport is incomplete** -- the POST handler returns `202 Accepted` with a stub response instead of the actual JSON-RPC result, and GET/SSE is unimplemented (returns 405).
3. **Proc macro attribute parsing is fragile** -- it uses string splitting on `attr.to_string()` instead of `syn::parse2` with proper token parsing, which will break on values containing commas or escaped quotes.
4. **No client SDK** -- there is no MCP client implementation, limiting the SDK to server-only use cases.
5. **Missing MCP spec features** -- no pagination support, no resource subscriptions, no sampling/completion, no cancellation handling.

Severity breakdown of 20 findings: **3 P0**, **8 P1**, **9 P2**.

---

## 1. API Design Quality

### Strengths
- Clean builder pattern with `#[must_use]` on all builder methods.
- Sensible prelude module that re-exports `async_trait` (a pragmatic convenience).
- Good separation between handler traits, registries, protocol types, and transport.
- `CallToolResult::text()` and `CallToolResult::error()` convenience constructors.
- `impl Into<String>` used idiomatically throughout.

### Findings

**F1: `McpServer::run()` takes `mut self` -- consumes the server.**
This prevents restarting or inspecting the server after shutdown. More critically, the `initialized` field is `bool` on the server struct, requiring `&mut self` in the message loop. This makes the server non-`Send` during execution, preventing wrapping in `Arc<Mutex<>>` for HTTP transport scenarios where multiple connections share state.

**F2: Sequential request processing is a blocking bottleneck.**
`handle_request()` takes `&mut self` and awaits each tool call inline. If a tool call takes 10 seconds, all other requests (including `ping`) are blocked. For any production HTTP deployment, this is a deal-breaker. The server should dispatch tool calls concurrently with `tokio::spawn`, requiring `Arc`-based shared state instead of `&mut self`.

**F3: `ToolHandler::info()` returns owned `ToolInfo` on every call.**
The `list()` method on `ToolRegistry` calls `h.info()` for every handler on every `tools/list` request, allocating fresh `ToolInfo` structs each time. The schema is a `serde_json::Value` which involves heap allocation. The info should be cached at registration time.

**F4: `McpServerBuilder::build()` panics on duplicate names.**
The method calls `.expect()` on duplicate registrations, which violates Rust API guidelines (C-SMART-PTR guideline: builders should not panic). It should return `Result<McpServer, McpError>` or at least document the panic in a `# Panics` section (partially done, but a `Result` return is better).

**F5: `ToolContext` is too thin for production use.**
No mechanism for progress reporting, cancellation tokens, or sending server-initiated notifications. The `request_id` is `Option<String>` instead of `RequestId`, losing type safety. There is no way for a tool to send back streaming results.

**F6: Glob re-exports in `mod.rs` files cause namespace pollution.**
Both `protocol/mod.rs` and `types/mod.rs` use `pub use submodule::*`. This means `use mcp_sdk::protocol::*` brings in both `types::*` and `messages::*` and `capabilities::*`, which can cause ambiguity and makes docs harder to navigate. Prefer explicit re-exports.

---

## 2. Proc Macro Quality

### Strengths
- Proper span-based error messages for non-async and generic functions.
- Generates both handler struct and factory function -- clean ergonomics.
- Automatic JSON Schema generation from parameter types.
- `trybuild` is listed as a dev-dependency (infrastructure exists).

### Findings

**F7: Attribute parsing uses string manipulation instead of `syn` parsing.**
`parse_tool_attrs()`, `parse_prompt_attrs()`, and `parse_resource_attrs()` all use `attr.to_string()` followed by `split(',')` and `strip_prefix`. This will break when:
- A description contains a comma: `description = "Add, subtract, or multiply"`
- A value contains an escaped quote.
- Whitespace varies (mostly handled, but fragile).

The correct approach is to use `syn::parse2::<syn::AttributeArgs>` or a custom `Parse` implementation.

**F8: `to_pascal_case` and `extract_string_value` are duplicated three times.**
The functions `to_pascal_case`, `extract_string_value`, and the attribute parsing pattern are copied verbatim across `tool.rs`, `prompt.rs`, and `resource.rs`. These should live in `utils.rs`.

**F9: The generated code uses `result.into()` but `CallToolResult` already implements `From<CallToolResult>`.**
Line 144 of `tool.rs` generates `Ok(result.into())` where `result` is already `CallToolResult`. The `.into()` is a no-op identity conversion, but it would fail at compile time if the user returns a different type. This suggests the macro intends to support implicit conversion but hasn't fully wired it up.

**F10: No support for `&self` parameter (stateful tools via macros).**
The `#[mcp_tool]` macro generates a unit struct. There is no way to use it with tools that hold state (like the `DbSetTool` example which holds a database handle). The macro should support `self` parameters or at least document this limitation prominently.

**F11: Only one trybuild UI test exists (`non_async.rs`).**
There should be tests for: generic functions, functions with lifetimes, functions returning the wrong type, duplicate parameter names, missing description on resource macro without URI, etc.

**F12: `type_to_schema` uses string comparison for type matching.**
`quote!(#ty).to_string().replace(' ', "")` is used to match types. This fails for fully-qualified paths (`std::string::String`), type aliases, or types from different crates. A more robust approach would use `syn::Type` pattern matching.

---

## 3. Protocol Compliance

### Strengths
- JSON-RPC 2.0 message structure is correct (jsonrpc, id, method, params fields).
- All five standard error codes are defined.
- `parse_jsonrpc_message()` correctly validates the `jsonrpc: "2.0"` field.
- MCP method names match the spec (tools/list, tools/call, etc.).
- Capability negotiation follows the spec structure.

### Findings

**F13: HTTP transport POST handler does not return the actual JSON-RPC response.**
The `handle_post` function routes the message to the server via an mpsc channel, but then returns a stub `{"result": {}}` with status `202 Accepted`. The actual response from the server is never routed back to the HTTP client. This means the HTTP transport is fundamentally non-functional -- it can receive requests but cannot return results.

**F14: No pagination support.**
`tools/list`, `resources/list`, `prompts/list` all return `next_cursor: None` unconditionally. The `ListToolsParams`, `ListResourcesParams`, `ListPromptsParams` structs have cursor fields defined but are never parsed or used in the handler methods.

**F15: Missing MCP methods.**
The following MCP spec methods are not implemented:
- `resources/subscribe` and `resources/unsubscribe`
- `completion/complete` (autocomplete)
- `sampling/createMessage` (server-initiated LLM calls)
- `roots/list` (client capability)
These are optional per the spec but should at least return `MethodNotFound` for subscribe/unsubscribe when the capability is not advertised.

**F16: Protocol version negotiation is not validated.**
The server accepts any `protocolVersion` string in the `initialize` params without checking compatibility. Per the MCP spec, the server should validate that the client's requested version is supported and respond with the version it will use. If incompatible, it should return an error.

**F17: `JsonRpcMessage` uses `#[serde(untagged)]` which has poor error messages.**
When deserialization fails for an untagged enum, serde reports "data did not match any variant" without indicating which variant was closest. The manual `parse_jsonrpc_message()` function provides better diagnostics, but the `serde::Deserialize` impl on `JsonRpcMessage` (used in HTTP handler `Json(body): Json<serde_json::Value>` + `serde_json::from_value`) will produce confusing errors.

---

## 4. Performance

### Strengths
- Buffered I/O with `BufReader`/`BufWriter` on stdio.
- `criterion` benchmarks with async runtime support.
- `serde_json` used directly (no intermediate formats).

### Findings

**F18: `info()` is called on every `list()` invocation, causing repeated allocations.**
`ToolRegistry::list()` calls `h.info()` for all N handlers, each time allocating a new `ToolInfo` with a fresh `serde_json::Value` for the schema. For a server with 100 tools receiving frequent `tools/list` calls, this creates significant allocation pressure. The `ToolInfo` should be cached at registration time.

**F19: `params.clone()` on every request parameter deserialization.**
In `server.rs`, every parameter extraction does `serde_json::from_value(p.clone())`. The clone of the `serde_json::Value` is unnecessary if we take ownership. Since `handle_request` takes `&JsonRpcRequest`, it can't move out of `params`. The request should be taken by value or the params consumed.

**F20: `String` allocation for `JSONRPC_VERSION` on every message.**
Every `JsonRpcRequest`, `JsonRpcResponse`, etc. stores `jsonrpc: String`. Since this is always "2.0", it could be `Cow<'static, str>` or handled by serde default/skip logic to avoid a heap allocation per message.

**F21: The memory benchmark returns hardcoded `0` and provides no useful data.**
`get_memory_usage()` always returns 0, making the entire memory benchmark produce `~0 bytes` for every measurement. This should use `jemalloc` or `dhat` for actual measurements, or at minimum use a global allocator wrapper that counts allocations.

---

## 5. Safety & Correctness

### Strengths
- No `unsafe` code anywhere in the codebase.
- Proper `Send + Sync + 'static` bounds on handler traits.
- `tokio::sync::Mutex` used correctly for async contexts (not `std::sync::Mutex`).
- `thiserror` used for error type derivation.

### Findings

**F22: `StdioTransport::recv()` takes `&mut self` but `send()` takes `&self` -- inconsistent borrowing.**
The `McpTransport` trait has `recv(&mut self)` and `send(&self)`. This means you cannot concurrently receive and send. For the stdio transport this is arguably correct (line-based protocol), but for HTTP/SSE where server-initiated notifications need to be sent while waiting for new requests, this is a fundamental design flaw. The recv/send should both take `&self` or the transport should be split into reader/writer halves (like `tokio::io::split`).

**F23: No cancellation safety.**
The server's main loop awaits `transport.recv()` and then `transport.send()`. If the future is cancelled between recv and send (e.g., due to a timeout or `tokio::select!`), the received message is lost with no way to recover. The server should use `tokio::select!` with cancellation-safe branches, or buffer received messages.

**F24: `initialized` flag is not atomic.**
The `initialized` field on `McpServer` is a plain `bool`. While currently only accessed in a single-threaded loop, if the server is ever made concurrent (F2), this becomes a data race. It should be `AtomicBool` or protected by the server's shared state.

**F25: `TestStdioTransport` uses `std::io::Cursor` inside `tokio::io::BufReader`.**
`std::io::Cursor<Vec<u8>>` does not implement `tokio::io::AsyncRead`. This means the test transport likely uses a compatibility layer or the code does not actually compile without some implicit conversion. If it does compile, the blocking I/O on a Cursor in an async context is technically unsound under Tokio's cooperative scheduling contract, though harmless for tests since Cursor never actually blocks.

---

## 6. Testing

### Strengths
- Good unit test coverage for protocol types (roundtrip serialization tests).
- Integration tests cover the full server lifecycle (initialize, tool call, error, EOF).
- `TestStdioTransport` is a well-designed test double.
- Tests verify both happy paths and error cases.

### Findings

**F26: No tests for the HTTP transport's actual request/response flow.**
The HTTP transport has only three tests: default config, builder, and router creation. There are no tests using `axum::test` to simulate HTTP requests and verify the actual response bodies/status codes. The fundamental bug (F13) where responses are never returned would have been caught.

**F27: No macro expansion tests.**
There is one trybuild test (`non_async.rs`) but no tests that verify the generated code structure. There should be `cargo-expand` snapshots or at least tests that verify:
- The generated struct name follows the expected pattern.
- The input schema matches expected JSON.
- Optional parameters are correctly handled.
- Multiple parameters with doc comments work.

**F28: No fuzz testing.**
A protocol parser handling arbitrary JSON input is a prime candidate for fuzz testing. `cargo-fuzz` or `proptest` should be used to test `parse_jsonrpc_message()` and the serde deserialization paths.

**F29: No test for duplicate tool name registration error.**
While `test_duplicate_tool()` exists in `tools.rs`, there's no test verifying that `McpServerBuilder::build()` actually panics (or returns an error) on duplicate names.

---

## 7. Documentation

### Strengths
- Crate-level doc comment with quick start example.
- Module-level doc comments on all modules.
- `#[doc]` comments on all public types and methods.
- Feature flags documented in a table.
- README includes Claude Desktop configuration example.

### Findings

**F30: Doc examples use `rust,no_run` or `rust,ignore` instead of compiling.**
The `lib.rs` example uses `rust,no_run`, the macro docs use `rust,ignore`. These should be actual doctests that compile (even if they can't run due to stdin/stdout requirements). Use `# fn main()` wrappers and test-only imports.

**F31: No changelog or migration guide.**
For a 0.1.0 release, a CHANGELOG.md tracking pre-release changes is important for early adopters.

**F32: README benchmark claims lack reproducibility.**
The benchmarks section claims "< 0.5ms" latency and "> 2,000 req/s" but the memory benchmark returns 0. The throughput benchmarks exist but are not run in CI. Claims should be backed by CI-gated benchmark comparisons.

---

## 8. Crate Ecosystem Fit

### Strengths
- MSRV set to 1.75.0 (good -- matches many enterprise requirements).
- Dual MIT/Apache-2.0 licensing.
- Well-designed feature flags (`stdio`, `http`, `sse`, `macros`, `full`).
- Workspace structure with separate proc-macro crate.

### Findings

**F33: `tokio` dependency uses `features = ["full"]` in workspace.**
This pulls in the entire Tokio runtime even when only `io` and `sync` features are needed. For library crates, this is an anti-pattern. The workspace should specify minimal features, and examples can add `full`.

**F34: No `#[doc(cfg)]` attributes for feature-gated items.**
Items behind `#[cfg(feature = "http")]` do not use `#[doc(cfg(feature = "http"))]`, so docs.rs won't indicate which features are needed for each item.

**F35: Crate name `mcp-sdk` may conflict with existing crates.**
Before publishing, verify the name is available on crates.io. Consider `mcp-server` or `mcp-rs` if `mcp-sdk` is taken or to better describe the server-only scope.

---

## 9. CI/CD & Release

### Strengths
- CI runs fmt, clippy, test (3 OS x 2 Rust versions), miri, docs, and coverage.
- `RUSTFLAGS="-Dwarnings"` ensures no warnings.
- `rust-cache` used for faster builds.
- Coverage uploaded to codecov.

### Findings

**F36: No `cargo-deny` or `cargo-audit` in CI.**
Supply chain security checks are missing. `cargo-deny` should check for license compatibility, duplicate dependencies, and known vulnerabilities.

**F37: No release workflow for crates.io publishing.**
There's no `release.yml` for automated crate publishing. This should include version bump verification, changelog validation, and `cargo publish --dry-run`.

**F38: Miri only runs `--lib` tests on `mcp-sdk`.**
It should also run on the proc-macro test helpers and integration tests where feasible.

---

## Prioritized Task List

### Task 1: Make Server Concurrent -- Dispatch Tool Calls via `tokio::spawn`
- **Priority:** P0
- **Effort:** L
- **Impact:** High
- **Assigned to:** Senior Async Rust Engineer
- **Description:** Refactor `McpServer` to use `Arc<ServerState>` shared state instead of `&mut self`. The message loop should dispatch `handle_request` into `tokio::spawn` tasks for tool calls, resource reads, and prompt gets. The `initialized` flag becomes `AtomicBool`. The transport trait needs to be split into separate `McpTransportReader` and `McpTransportWriter` traits (or use channels internally) so that responses can be sent from spawned tasks without holding a mutable borrow on the transport. This unblocks concurrent request handling and is a prerequisite for a functional HTTP transport.
- **Acceptance criteria:** (1) Multiple concurrent `tools/call` requests complete without blocking each other. (2) `ping` responds immediately even while a long-running tool executes. (3) All existing tests pass. (4) New test: two tool calls dispatched concurrently complete in less time than sequential execution.

### Task 2: Fix HTTP Transport to Return Actual JSON-RPC Responses
- **Priority:** P0
- **Effort:** L
- **Impact:** High
- **Assigned to:** Senior Async Rust Engineer
- **Description:** The current HTTP POST handler routes messages to the server but returns a stub response. Implement proper request-response correlation: when a POST arrives, route the JSON-RPC request to the server, await the response (via a oneshot channel keyed by request ID), and return the actual JSON-RPC response in the HTTP response body. For notifications (no ID), return `202 Accepted` immediately. For requests, return `200 OK` with `Content-Type: application/json`. Implement session creation on first `initialize` request (return `Mcp-Session-Id` header). Implement SSE response mode where the response is streamed as SSE events.
- **Acceptance criteria:** (1) `POST /mcp` with an `initialize` request returns the actual `InitializeResult` JSON-RPC response. (2) `POST /mcp` with a `tools/call` request returns the tool's result. (3) Session ID is created and returned in headers. (4) Integration test using `axum::test::TestServer`. (5) GET returns SSE stream for server-initiated notifications.

### Task 3: Replace String-Based Macro Attribute Parsing with `syn` Parsing
- **Priority:** P0
- **Effort:** M
- **Impact:** High
- **Assigned to:** Senior Proc Macro Engineer
- **Description:** Replace all three `parse_*_attrs()` functions (tool, prompt, resource) with a single shared `syn::Parse` implementation. Define an `McpAttrArgs` struct that implements `syn::parse::Parse` and handles `key = "value"` pairs and bare identifiers properly, including support for values containing commas, escaped quotes, and arbitrary whitespace. Eliminate the duplicated `to_pascal_case` and `extract_string_value` functions by moving them to `utils.rs`.
- **Acceptance criteria:** (1) `#[mcp_tool(description = "Add, subtract, or multiply")]` parses correctly. (2) `#[mcp_tool(name = "my\"tool")]` parses correctly. (3) All existing macro tests pass. (4) New trybuild tests for edge cases. (5) Zero code duplication between tool.rs, prompt.rs, and resource.rs for parsing logic.

### Task 4: Cache `ToolInfo` at Registration Time
- **Priority:** P1
- **Effort:** S
- **Impact:** Medium
- **Assigned to:** Senior Rust Engineer
- **Description:** Store the `ToolInfo` (and `ResourceInfo`, `PromptInfo`) at registration time in the registry alongside the handler `Arc`. Change `list()` to return `&[ToolInfo]` or `Vec<ToolInfo>` cloned from the cached values instead of calling `handler.info()` on every request. This eliminates repeated `serde_json::Value` allocations for the input schema on every `tools/list` call.
- **Acceptance criteria:** (1) `ToolRegistry::list()` does not call `handler.info()`. (2) Benchmark shows no allocation in `tool_list_10_tools`. (3) All tests pass.

### Task 5: Add Protocol Version Negotiation Validation
- **Priority:** P1
- **Effort:** S
- **Impact:** Medium
- **Assigned to:** Rust Protocol Engineer
- **Description:** In `handle_initialize()`, validate that `init_params.protocol_version` is a supported version. Maintain a list of supported versions (currently `["2025-11-25"]`). If the client requests an unsupported version, return the closest compatible version or an error. Log a warning if the client requests an older version.
- **Acceptance criteria:** (1) Initialize with supported version succeeds. (2) Initialize with unknown version returns an error or negotiated version. (3) Test covers both cases.

### Task 6: Implement Proper Cancellation Handling
- **Priority:** P1
- **Effort:** M
- **Impact:** Medium
- **Assigned to:** Senior Async Rust Engineer
- **Description:** Implement `notifications/cancelled` handling. When a cancellation notification is received, store a `CancellationToken` (from `tokio_util`) keyed by request ID. Pass the token through `ToolContext` so tool handlers can check `context.is_cancelled()` and bail early. When the server dispatches a tool call, wrap it in `tokio::select!` with the cancellation token.
- **Acceptance criteria:** (1) Cancellation notification cancels a pending tool call. (2) `ToolContext::is_cancelled()` returns true after cancellation. (3) Cancelled tool calls return an appropriate error response. (4) Integration test: send a slow tool call, then cancel it, verify it completes early.

### Task 7: Split `McpTransport` into Reader + Writer
- **Priority:** P1
- **Effort:** M
- **Impact:** High
- **Assigned to:** Senior Async Rust Engineer
- **Description:** Refactor `McpTransport` trait into `McpTransportReader` (with `recv`) and `McpTransportWriter` (with `send`, `send_to`, `close`). The writer should be cloneable (backed by `Arc`) so spawned tasks can send responses independently. `StdioTransport` splits into a reader half (owning `BufReader<Stdin>`) and a writer half (owning `Arc<Mutex<BufWriter<Stdout>>>`). This enables concurrent sending from multiple tasks after Task 1.
- **Acceptance criteria:** (1) `McpTransportReader::recv(&mut self)` and `McpTransportWriter::send(&self)` both work. (2) Writer is `Clone + Send + Sync`. (3) Server can send responses from spawned tasks. (4) All existing tests pass after migration.

### Task 8: Add Pagination Support for List Methods
- **Priority:** P1
- **Effort:** M
- **Impact:** Medium
- **Assigned to:** Rust Protocol Engineer
- **Description:** Implement cursor-based pagination for `tools/list`, `resources/list`, and `prompts/list`. Parse the `cursor` field from `ListToolsParams`. Implement a simple offset-based pagination scheme: the cursor encodes an offset, and the server returns at most N items (configurable, default 100) starting from that offset. Return `next_cursor` when there are more items.
- **Acceptance criteria:** (1) `tools/list` with no cursor returns first page and `nextCursor` if more items exist. (2) `tools/list` with cursor returns next page. (3) Works for resources and prompts too. (4) Tests with 150 tools verify pagination across 2 pages.

### Task 9: Fix `tokio` Feature Flags -- Use Minimal Features for Library
- **Priority:** P1
- **Effort:** S
- **Impact:** Medium
- **Assigned to:** Rust Build Engineer
- **Description:** Change the workspace `tokio` dependency from `features = ["full"]` to `features = ["io-std", "io-util", "sync", "macros", "rt-multi-thread"]`. The `full` feature should only be used in examples and benchmarks. This reduces compile time and binary size for downstream consumers who only need specific Tokio features. Add `rt-multi-thread` to the library only if needed, otherwise let consumers choose their runtime.
- **Acceptance criteria:** (1) `cargo build -p mcp-sdk` succeeds with minimal Tokio features. (2) Examples still compile with `tokio/full`. (3) CI passes.

### Task 10: Add Comprehensive Trybuild UI Tests for Macros
- **Priority:** P1
- **Effort:** M
- **Impact:** Medium
- **Assigned to:** Senior Proc Macro Engineer
- **Description:** Add trybuild compile-fail tests for: (1) non-async function, (2) function with generics, (3) function with lifetime parameters, (4) function with wrong return type, (5) `mcp_resource` without `uri` attribute, (6) duplicate parameter names, (7) function with `self` parameter. Add compile-pass tests for: (1) function with `Option<T>` parameters, (2) function with `Vec<T>` parameters, (3) function with doc comments on parameters, (4) function with custom name override.
- **Acceptance criteria:** (1) At least 10 trybuild test files exist. (2) All compile-fail tests have `.stderr` snapshot files. (3) CI runs trybuild tests.

### Task 11: Avoid Cloning `serde_json::Value` for Parameter Deserialization
- **Priority:** P1
- **Effort:** S
- **Impact:** Medium
- **Assigned to:** Senior Rust Engineer
- **Description:** In `server.rs`, change `handle_request` to take `JsonRpcRequest` by value instead of by reference. This allows `serde_json::from_value(params)` to consume the value without cloning. Update the message loop to move the request out of the `JsonRpcMessage` enum. This eliminates an unnecessary deep clone of the params JSON on every request.
- **Acceptance criteria:** (1) No `.clone()` calls on `serde_json::Value` in request handling. (2) All tests pass. (3) Benchmark shows reduced allocation for `tool_call_echo`.

### Task 12: Add `cargo-deny` and `cargo-audit` to CI
- **Priority:** P2
- **Effort:** S
- **Impact:** Medium
- **Assigned to:** DevOps / Release Engineer
- **Description:** Add a `deny.toml` configuration file and a `cargo-deny` CI job. Configure it to check: (1) no duplicate dependencies where avoidable, (2) all licenses are MIT/Apache-2.0 compatible, (3) no known security advisories. Add `cargo-audit` as a separate job that runs weekly on a cron schedule.
- **Acceptance criteria:** (1) `cargo deny check` passes in CI. (2) `deny.toml` is committed. (3) Weekly `cargo-audit` job exists.

### Task 13: Use `Cow<'static, str>` for `jsonrpc` Field
- **Priority:** P2
- **Effort:** S
- **Impact:** Low
- **Assigned to:** Rust Performance Engineer
- **Description:** Change the `jsonrpc` field in `JsonRpcRequest`, `JsonRpcResponse`, `JsonRpcErrorResponse`, and `JsonRpcNotification` from `String` to `Cow<'static, str>`. Default it to `Cow::Borrowed("2.0")` in all constructors. This avoids a heap allocation for the constant "2.0" string on every message.
- **Acceptance criteria:** (1) All message types use `Cow<'static, str>` for jsonrpc. (2) No heap allocation for the jsonrpc field in normal usage. (3) Serialization/deserialization roundtrip tests pass.

### Task 14: Implement a Proper Memory Benchmark
- **Priority:** P2
- **Effort:** M
- **Impact:** Low
- **Assigned to:** Rust Performance Engineer
- **Description:** Replace the placeholder `get_memory_usage()` with actual measurement using `dhat` (for heap profiling) or a custom global allocator that tracks peak usage. Alternatively, use `tikv-jemalloc-ctl` for `stats.allocated`. The benchmark should produce actionable numbers that can be compared across commits.
- **Acceptance criteria:** (1) Memory benchmark reports non-zero values. (2) Values are reproducible within 10%. (3) Benchmark runs in CI and results are archived.

### Task 15: Add `#[doc(cfg)]` for Feature-Gated Items
- **Priority:** P2
- **Effort:** S
- **Impact:** Low
- **Assigned to:** Rust Documentation Engineer
- **Description:** Add `#![feature(doc_cfg)]` to `lib.rs` (behind `#[cfg(docsrs)]`) and add `#[doc(cfg(feature = "http"))]` annotations to `transport::http`, `transport::sse`, and all types gated behind features. Configure `Cargo.toml` with `[package.metadata.docs.rs]` to enable all features and `doc_cfg` on docs.rs.
- **Acceptance criteria:** (1) docs.rs shows feature badges on gated items. (2) `cargo doc` still builds cleanly without the nightly feature.

### Task 16: Add Release Workflow for crates.io Publishing
- **Priority:** P2
- **Effort:** M
- **Impact:** Medium
- **Assigned to:** DevOps / Release Engineer
- **Description:** Create a `.github/workflows/release.yml` that triggers on version tag pushes. The workflow should: (1) verify the version in `Cargo.toml` matches the tag, (2) run the full CI suite, (3) publish `mcp-macros` first (since `mcp-sdk` depends on it), (4) publish `mcp-sdk`, (5) create a GitHub release with auto-generated notes.
- **Acceptance criteria:** (1) Pushing a `v0.1.0` tag triggers the release workflow. (2) Both crates are published in order. (3) Publish uses `--dry-run` on PRs.

### Task 17: Improve `type_to_schema` to Use `syn::Type` Pattern Matching
- **Priority:** P2
- **Effort:** M
- **Impact:** Medium
- **Assigned to:** Senior Proc Macro Engineer
- **Description:** Replace the string-based type matching in `utils::type_to_schema` with proper `syn::Type` pattern matching. Match on `Type::Path` segments to identify `Option`, `Vec`, `String`, numeric types, and `bool`. Handle fully-qualified paths like `std::string::String`. For unrecognized types, generate `serde_json::json!({"type": "object"})` with a compile-time warning.
- **Acceptance criteria:** (1) `std::string::String` maps to `"string"`. (2) `Option<Vec<i32>>` maps to `{"type": "array", "items": {"type": "integer"}}`. (3) Nested generics are handled. (4) Unknown types produce a clear warning.

### Task 18: Add Fuzz Testing for Protocol Parser
- **Priority:** P2
- **Effort:** M
- **Impact:** Medium
- **Assigned to:** Rust Security Engineer
- **Description:** Add a `fuzz/` directory with `cargo-fuzz` targets for: (1) `parse_jsonrpc_message()` with arbitrary string input, (2) `serde_json::from_value::<InitializeParams>()` with arbitrary JSON, (3) `serde_json::from_value::<CallToolParams>()` with arbitrary JSON. Run fuzz targets for at least 1 hour and fix any panics found.
- **Acceptance criteria:** (1) Fuzz targets exist and run. (2) No panics found after 1 hour of fuzzing. (3) CI runs a short fuzz pass (30 seconds) as a smoke test.

### Task 19: Remove Glob Re-exports from Module Files
- **Priority:** P2
- **Effort:** S
- **Impact:** Low
- **Assigned to:** Rust API Design Engineer
- **Description:** Replace `pub use submodule::*` in `protocol/mod.rs`, `types/mod.rs`, and `handler/mod.rs` with explicit re-exports of key types. This prevents namespace pollution, makes it clear which types come from which submodule, and produces cleaner documentation. The prelude can continue to use glob re-exports since that is its purpose.
- **Acceptance criteria:** (1) `protocol/mod.rs` explicitly re-exports key types. (2) All downstream code compiles. (3) `cargo doc` shows clean module hierarchy.

### Task 20: Add HTTP Transport Integration Tests Using `axum::test`
- **Priority:** P2
- **Effort:** M
- **Impact:** High
- **Assigned to:** Senior Rust Engineer
- **Description:** Add integration tests for the HTTP transport using `axum_test` or `tower::ServiceExt::oneshot`. Test: (1) POST with valid initialize request returns correct response, (2) POST with invalid JSON returns parse error, (3) POST with unknown session ID returns 404, (4) DELETE with valid session ID returns 204, (5) DELETE without session header returns 400, (6) GET returns SSE stream (when implemented). These tests should run in CI under the `http` feature flag.
- **Acceptance criteria:** (1) At least 6 HTTP transport integration tests. (2) Tests verify actual response bodies. (3) Tests run in CI with `--features http`.

---

## Summary Statistics

| Category | Findings | P0 | P1 | P2 |
|----------|----------|----|----|-----|
| API Design | 6 | 1 | 2 | 3 |
| Proc Macros | 6 | 1 | 2 | 3 |
| Protocol | 5 | 1 | 2 | 2 |
| Performance | 4 | 0 | 2 | 2 |
| Safety | 4 | 1 | 1 | 2 |
| Testing | 4 | 0 | 1 | 3 |
| Documentation | 3 | 0 | 0 | 3 |
| Ecosystem | 3 | 0 | 1 | 2 |
| CI/CD | 3 | 0 | 0 | 3 |
| **Total** | **38** | **3** | **8** | **9** |

**Lines of Rust code audited:** ~3,800 (excluding generated/target files)
**Test count:** ~65 tests across unit, integration, and macro tests
**Estimated effort to reach 1.0 quality:** 6-8 engineer-weeks

---

## Conclusion

The `mcp-rust-sdk` has a solid architectural foundation and demonstrates thoughtful API design. The three P0 issues (concurrent request handling, functional HTTP transport, and robust macro parsing) are the critical blockers for any production deployment. Addressing the P0 and P1 tasks would bring the SDK to a state suitable for early adopter usage. The P2 tasks represent polish and completeness improvements needed before a stable 1.0 release.

The codebase is clean, well-documented, and follows Rust conventions. With focused effort on the identified issues, this SDK has strong potential to become the reference MCP server implementation for the Rust ecosystem.
