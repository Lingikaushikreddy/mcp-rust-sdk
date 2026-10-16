# Recorded wire sessions

Golden transcripts of real MCP clients talking to the conformance fixture
(`examples/conformance`). `tests/wire_replay.rs` replays each client message
and asserts the server's responses are unchanged.

| File | Client | Transport |
|---|---|---|
| `inspector.stdio.jsonl` | MCP Inspector CLI 2.11.0 | stdio |
| `inspector.http.jsonl` | MCP Inspector CLI 2.11.0 | Streamable HTTP |
| `claude.stdio.jsonl` | Claude Code 2.1.301 | stdio |
| `claude.http.jsonl` | Claude Code 2.1.301 | Streamable HTTP |

Claude Code opens with a `server/discover` probe (protocol 2026-07-28); the
recordings show it falling back to `initialize` after the server rejects the
probe, which is the downgrade path the newer specification defines.

stdio files hold `{"dir": "c2s"|"s2c", "msg": ...}` lines captured by a tee
between client and server. HTTP files hold one object per HTTP exchange
(`method`, MCP-relevant request and response headers, `status`, and the JSON
bodies) captured by a logging reverse proxy.

When a behavior change is intentional, re-record instead of editing by hand:
run the clients against the fixture through the tee/proxy and replace these
files, then review the diff.
