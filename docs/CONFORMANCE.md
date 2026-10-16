# MCP 2025-11-25 conformance

This is the conformance record for `mcp-sdk` against the **2025-11-25** revision
of the Model Context Protocol, the revision the SDK negotiates by default
(`PROTOCOL_VERSION`). It was produced on 2026-10-15 from the specification
text, not from this repository's own documents.

**Sources** (pinned):

- Spec text and `schema.json`: [modelcontextprotocol/modelcontextprotocol](https://github.com/modelcontextprotocol/modelcontextprotocol)
  at `86f6e0c`, directory `docs/specification/2025-11-25/`. Citations below
  are `file:line` inside that directory.
- JSON-RPC 2.0 specification, section 5.1 (error codes).
- Official conformance suite: [modelcontextprotocol/conformance](https://github.com/modelcontextprotocol/conformance)
  at `c37eec8` (0.2.0-alpha.12), run with `--requirements 2025-11-25`.

A newer revision, **2026-07-28**, exists. It is out of scope here; see
[Newer revision](#newer-revision).

## Summary

| | Before | After |
|---|---|---|
| Official suite, 2025-11-25 requirements (checks) | 48 passed, 13 failed | 62 passed, 10 failed |
| Official suite failures that are spec violations | 2 (`tools-call-error`, `dns-rebinding-protection`) | 0 |
| Wire messages failing `schema.json` (stdio / HTTP probes) | 0 / 7 | 0 / 0 |
| Non-conformant rows in the matrix below | 20 | 1 (deliberate, see R-3) |

The 10 remaining suite failures are all features this SDK does not implement
(completion, resource subscriptions, audio content, log and progress
notifications, sampling, elicitation). They are listed as **not implemented**
rows, not as defects.

**Ratings**

- **Conformant**: behavior matches the cited requirement, with evidence.
- **Non-conformant**: behavior contradicts a MUST/SHOULD. Every such row was fixed unless noted.
- **Not implemented**: optional feature the SDK does not offer. Where the spec says how to decline (for example `405` for GET), declining correctly is conformant.

**Evidence**

- `T:` test in this repository
- `S:` official conformance suite scenario
- `P:` black-box probe of the conformance fixture
- `R:` replay of a session recorded from MCP Inspector 2.11.0 or Claude Code 2.1.301 (`examples/conformance/tests/wire/`)

## Matrix

### Lifecycle and initialization

| # | Requirement | Spec | Before | After | Evidence |
|---|---|---|---|---|---|
| L-1 | Server responds to `initialize` with its capabilities and info (MUST) | lifecycle.mdx:98 | Conformant | Conformant | S: server-initialize; R: all four |
| L-2 | If the requested version is supported, respond with the same version (MUST) | lifecycle.mdx:170 | Conformant | Conformant | T: `test_protocol_version_validation`; P: init.latest |
| L-3 | Otherwise respond with another supported version, SHOULD be the latest (MUST/SHOULD) | lifecycle.mdx:171-172 | Conformant | Conformant | P: init.unknown-version, init.2025-06-18 return `2025-11-25` |
| L-4 | Ping before `initialize` is permitted; ping MUST be answered | lifecycle.mdx:157-159, utilities/ping.mdx:29 | **Non-conformant**: rejected with -32600 | Conformant | T: `ping_before_initialize_is_answered`; S: ping |
| L-5 | Other requests before initialization | lifecycle.mdx:157 (client SHOULD NOT) | Rejected with -32600 | Unchanged | R: Claude Code's `server/discover` probe is rejected, then it initializes |

### Capabilities

| # | Requirement | Spec | Before | After | Evidence |
|---|---|---|---|---|---|
| C-1 | Servers that support tools MUST declare `tools` | server/tools.mdx:38 | Conformant | Conformant | T: `test_capabilities_reflect_registrations` |
| C-2 | `listChanged` advertised only if notifications are sent | server/tools.mdx:50 | Conformant (`false`) | Conformant | S: tools-list |
| C-3 | `logging` declared only by servers that emit log notifications (MUST) | server/utilities/logging.mdx:19 | Conformant (not declared; `logging/setLevel` is acknowledged) | Conformant | S: logging-set-level |
| C-4 | Completions, resource subscriptions, tasks | lifecycle.mdx:191-204 | Not implemented | Not implemented | S: completion-complete, resources-subscribe fail |

### Base protocol (JSON-RPC)

| # | Requirement | Spec | Before | After | Evidence |
|---|---|---|---|---|---|
| J-1 | Responses carry the request's id (MUST) | basic/index.mdx:69 | **Non-conformant** over HTTP: two sessions using the same id got each other's results | Conformant | T: `same_request_id_in_two_sessions_is_routed_to_each_session`; P: collision |
| J-2 | Request ids MUST NOT be `null` | basic/index.mdx:47 | **Non-conformant**: answered as a parse error with `id: 0` | Conformant: -32600 | T: `non_request_json_gets_invalid_request_without_an_id` |
| J-3 | Errors for unreadable requests carry no id (`id?: RequestId`) | basic/index.mdx:89, schema.ts `JSONRPCErrorResponse` | **Non-conformant**: `id: 0` (stdio) and `id: null` (HTTP) | Conformant: id omitted | T: `invalid_json_gets_a_parse_error_without_an_id`, `invalid_message_error_body_has_no_id`; P: 0 schema violations |
| J-4 | Invalid JSON is -32700; JSON that is not a valid Request is -32600 | JSON-RPC 2.0 section 5.1 | **Non-conformant**: everything was -32700 | Conformant | T: same as J-2/J-3 |
| J-5 | Batches are not part of MCP (removed after 2025-03-26) | schema.json `JSONRPCMessage` | **Non-conformant** over HTTP: `jsonrpc: "1.0"` and other invalid envelopes were accepted | Conformant: -32600 | T: `invalid_message_error_body_has_no_id`; P: malformed.batch-array |
| J-6 | Notifications get no response (MUST NOT) | basic/index.mdx:96 | Conformant (stdio) | Conformant | P: tools.unexpected-after-unknown-notification |
| J-7 | A handler failure is still answered with that request's id | basic/index.mdx:89 | **Non-conformant**: a panicking tool was answered with `id: 0` | Conformant | T: `panicking_tool_is_answered_with_its_own_request_id` |
| J-8 | Every message validates against `schema.json` | schema.json | **Non-conformant**: 7 HTTP bodies invalid | Conformant | P: schema validation of every server message; S: `wire-schema-valid` on every scenario |

### Tools

| # | Requirement | Spec | Before | After | Evidence |
|---|---|---|---|---|---|
| T-1 | `tools/list` returns tool definitions with `inputSchema` objects | server/tools.mdx:195-213 | Conformant | Conformant | S: tools-list, json-schema-2020-12; R: all four |
| T-2 | Annotations (`title`, four hints) serialized as given | server/tools.mdx:208-213, schema.ts `ToolAnnotations` | Conformant | Conformant | T: `tool_annotations_test`; P: tools.list.echo.annotations |
| T-3 | Unknown tool is a protocol error (-32602) | server/tools.mdx:464-466, 486 | Conformant | Conformant | T: `unknown_tool_is_an_invalid_params_protocol_error` |
| T-4 | Execution errors (API failures, business logic) are results with `isError: true` | server/tools.mdx:469-473 | **Non-conformant**: JSON-RPC -32603 | Conformant | T: `tool_execution_error_is_an_is_error_result`; S: tools-call-error; R: both clients now see `boom` as a tool result |
| T-5 | Input validation errors are results with `isError: true` | server/tools.mdx:471 | **Non-conformant**: JSON-RPC -32602 | Conformant | T: `tool_input_validation_error_is_an_is_error_result` |
| T-6 | Text, image and embedded resource content | server/tools.mdx:230-320 | Conformant | Conformant | S: tools-call-simple-text, -image, -embedded-resource, -mixed-content |
| T-7 | Audio content, resource links, `structuredContent`/`outputSchema`, tool `title`/`icons` | server/tools.mdx:205, 324 | Not implemented | Not implemented | S: tools-call-audio fails |
| T-8 | Log and progress notifications during a call, sampling, elicitation | utilities/progress.mdx, client/*.mdx | Not implemented | Not implemented | S: tools-call-with-logging, -with-progress, -sampling, -elicitation fail |

### Resources and prompts

| # | Requirement | Spec | Before | After | Evidence |
|---|---|---|---|---|---|
| R-1 | `resources/list`, `resources/read` (text and blob), templates | server/resources.mdx | Conformant | Conformant | S: resources-list, -read-text, -read-binary, -templates-read |
| R-2 | `prompts/list`, `prompts/get` with arguments, images, embedded resources | server/prompts.mdx | Conformant | Conformant | S: prompts-list, prompts-get-* |
| R-3 | Resource not found SHOULD be -32002 | server/resources.mdx:386-388 | Non-conformant (-32602) | **Non-conformant, deliberately unchanged** | The next revision (SEP-2164) standardizes on -32602 and the suite already checks for -32602, so changing it would be reverted |
| R-4 | Pagination | server/utilities/pagination.mdx | Not implemented (`nextCursor` never set; optional) | Not implemented | |

### Cancellation and ping

| # | Requirement | Spec | Before | After | Evidence |
|---|---|---|---|---|---|
| X-1 | Ping MUST be answered promptly, including while other requests run | utilities/ping.mdx:29 | **Non-conformant**: ping waited for in-flight tool calls (1.5 s in the probe) | Conformant (0 ms) | T: `ping_is_answered_while_a_tool_call_runs`; P: concurrency |
| X-2 | On `notifications/cancelled` SHOULD stop processing and SHOULD NOT respond | utilities/cancellation.mdx:37-40 | **Non-conformant**: the notification was read only after the request finished; it ran to completion and was answered | Conformant | T: `cancelled_request_gets_no_response`, `cancelled_request_post_is_released_without_a_response`; P: cancellation |
| X-3 | Unknown or completed request ids in a cancellation are ignored | utilities/cancellation.mdx:41-44, 77-81 | Conformant | Conformant | Code path: `cancel_request` ignores unknown ids |

### stdio transport

| # | Requirement | Spec | Before | After | Evidence |
|---|---|---|---|---|---|
| S-1 | Newline-delimited messages with no embedded newlines; UTF-8 | basic/transports.mdx:7, 28 | Conformant | Conformant | R: Inspector and Claude Code stdio |
| S-2 | Nothing but valid MCP messages on stdout (MUST NOT) | basic/transports.mdx:33 | Conformant | Conformant | Fuzz: `stdio_session` checks every stdout line |
| S-3 | Server exits when the client closes stdin | lifecycle.mdx:230-236 | Conformant (exit 0) | Conformant; in-flight requests finish first | P: exit code per session |

### Streamable HTTP transport

| # | Requirement | Spec | Before | After | Evidence |
|---|---|---|---|---|---|
| H-1 | Validate `Origin`; invalid → 403 (MUST) | basic/transports.mdx:78-80 | **Non-conformant**: no check | Conformant: Host and Origin checked against an allow-list | T: `foreign_origin_is_forbidden`, `foreign_host_is_forbidden`; S: dns-rebinding-protection |
| H-2 | Bind to localhost when running locally (SHOULD) | basic/transports.mdx:81 | **Non-conformant**: default `0.0.0.0` | Conformant: `127.0.0.1` | T: `default_bind_address_is_localhost` |
| H-3 | Accepted notification or response → 202 with no body (MUST) | basic/transports.mdx:95-97 | **Non-conformant**: body `{"id":null,"result":{}}` | Conformant | T: `accepted_notification_gets_202_with_no_body` |
| H-4 | Request → `application/json` or `text/event-stream` (MUST) | basic/transports.mdx:101-104 | Conformant (JSON) | Conformant (JSON) | R: both clients |
| H-5 | Unacceptable input → HTTP error, body MAY be a JSON-RPC error with no id | basic/transports.mdx:98-100 | **Non-conformant**: plain-text body; JSON bodies had `id: null` | Conformant | T: `invalid_message_error_body_has_no_id` |
| H-6 | GET → SSE stream or 405 (MUST) | basic/transports.mdx:140-142 | Conformant (405) | Conformant (405 with `Allow`) | P: get; R: both clients accept the 405 |
| H-7 | Session id returned in `MCP-Session-Id` on the InitializeResult response | basic/transports.mdx:198-200 | **Non-conformant**: created but never sent | Conformant | T: `initialize_response_carries_session_id`; S: server-session-lifecycle |
| H-8 | Session id is visible ASCII, cryptographically secure (MUST/SHOULD) | basic/transports.mdx:201-204 | Conformant (UUIDv4) | Conformant | T: same |
| H-9 | Requests without a session id (other than initialize) → 400 (SHOULD) | basic/transports.mdx:209-210 | **Non-conformant**: accepted | Conformant | T: `request_without_session_id_is_rejected` |
| H-10 | Terminated session → 404 (MUST) | basic/transports.mdx:211-212 | Conformant for unknown ids, but sessions could not be used | Conformant | T: `deleted_session_gets_404` |
| H-11 | DELETE ends a session | basic/transports.mdx:216-220 | Conformant (204) | Conformant | T: same |
| H-12 | Unsupported `MCP-Protocol-Version` → 400 (MUST) | basic/transports.mdx:279-280 | **Non-conformant**: accepted | Conformant | T: `unsupported_protocol_version_header_is_400`; R: Claude Code's 2026-07-28 probe gets 400 and it falls back |
| H-13 | Missing `MCP-Protocol-Version` is accepted (SHOULD assume 2025-03-26) | basic/transports.mdx:274-277 | Conformant | Conformant | T: `missing_protocol_version_header_is_accepted` |
| H-14 | Server-initiated messages, SSE streams, resumability | basic/transports.mdx:105-190 | Not implemented | Not implemented | Requests are answered with JSON only |
| H-15 | Authorization (OAuth 2.1) for HTTP transports (SHOULD) | basic/authorization.mdx | Not implemented | Not implemented | |

## Cross-SDK comparison

The same probes were run against the official Rust SDK (`rmcp` 3.5.1) and the
TypeScript SDK (`@modelcontextprotocol/sdk` 1.32.1) serving identical
`echo`/`slow`/`fail` tools.

| Behavior | mcp-sdk before | mcp-sdk after | rmcp 3.5.1 | TypeScript 1.32.1 |
|---|---|---|---|---|
| Tool execution error | -32603 error | `isError` result | `isError` result | `isError` result |
| Input validation error | -32602 error | `isError` result | `isError` result | `isError` result |
| Unknown tool | -32602 | -32602 | -32602 | `isError` result |
| Ping during a 1.5 s call | 1504 ms | 0 ms | 0 ms | 3 ms |
| Cancelled request | answered | no response | no response | no response |
| Ping before initialize | -32600 | answered | answered | answered |
| Invalid JSON on stdio | -32700, `id: 0` | -32700, no id | no reply | no reply |
| Initialize with `2025-06-18` | `2025-11-25` | `2025-11-25` | `2025-06-18` | `2025-06-18` |
| Foreign `Host` | 200 | 403 | 403 | 403 |
| Foreign `Origin`, local `Host` | 200 | 403 | 200 | 200 |
| Unsupported version header | 200 | 400 | 400 | 400 |
| Request without session id | 200 | 400 | 422 | 400 |
| Same id in two sessions | results swapped | correct | correct | correct |
| 202 body | non-empty | empty | empty | empty |
| HTTP error body id | `null` | absent | plain text | `null` |
| POST response format | JSON | JSON | SSE | SSE |

Neither official SDK rejects a foreign `Origin` that arrives with a local
`Host`, and the TypeScript SDK also sends `"id": null` in HTTP error bodies,
which the 2025-11-25 schema does not allow. These are noted here as
observations, not reported upstream.

## Fuzzing

`fuzz/` holds three `cargo-fuzz` targets. Each ran for 15 minutes (900 s) on
2026-10-16 with seeds taken from the recorded client sessions:

| Target | Checks | Runs | Result |
|---|---|---|---|
| `stdio_session` | Arbitrary stdin never crashes or hangs the server; every stdout line is a valid JSON-RPC message (basic/transports.mdx:33) | 4.08 M | No failures |
| `http_post` | Arbitrary bodies, session ids, version headers, `Origin` and `Host` never crash or hang a POST; 202 has no body; every other body is a valid JSON-RPC message | 19.4 M | No failures |
| `decode_message` | The decoder never panics, and an accepted message survives encode then decode unchanged | 24.5 M | **1 finding**, below |

**Finding: float precision.** `serde_json`'s default number parser is not
correctly rounded. Parsing the integer literal `1111111111111111111111118` and
then re-parsing the shortest output, `1.1111111111111111e24`, gives values that
differ by one unit in the last place. Some float literals sent by a client
therefore reach a tool's `arguments` off by 1 ULP. This does not violate MCP or
JSON (RFC 8259 leaves number precision to implementations), so it is not fixed
here. Enabling `serde_json`'s `float_roundtrip` feature in `mcp-sdk` removes
it: the same input passes with the feature on. The `decode_message` target
keeps the strict round-trip check, so it reproduces the finding until the
feature is enabled.

## Newer revision

The 2026-07-28 revision replaces the initialize handshake with a stateless
lifecycle (`server/discover`, per-request `_meta`). This SDK does not
implement it. Clients that speak it, such as Claude Code 2.1.301, first send a
2026-07-28 probe; the server rejects it (HTTP 400, or a JSON-RPC error on
stdio) and the client falls back to `initialize`, which is the downgrade path
`2026-07-28/basic/versioning.mdx` defines.

## Reproducing

- In-repo: `cargo test --workspace --all-features` runs the `T:` and `R:` evidence.
- Official suite: run `cargo run -p mcp-example-conformance -- http 8091`, then
  `npx @modelcontextprotocol/conformance server --url http://127.0.0.1:8091/mcp --requirements 2025-11-25`
  (built from source at the pinned commit; the npm `latest` tag predates `--requirements`).
- CI: the `MCP Conformance (2025-11-25)` job runs the official suite on every
  pull request, with `examples/conformance/expected-failures.yml` as the baseline.
- Fuzzing: `cd fuzz && cargo +nightly fuzz run <target> -- -max_total_time=900`.
