# vue_vet_mcp

Thin **MCP** adapter over [`vue_vet_session`](../vue_vet_session/README.md).

Stdio JSON-RPC with Content-Length framing. Exposes scan, explain,
explain-scope, and safe-fix **preview** tools under workspace path bounds.

Does **never** apply edits — apply remains CLI / LSP.

## Status

Workspace-internal (`publish = false`). Entry: `vue-vet --mcp` →
`run_stdio(workspace_root)`.

## Tools

Exact names in `TOOL_NAMES`:

| Tool | Role |
| --- | --- |
| `vue_vet_scan` | Analyze path; JSON v1 including CLI-shaped `reactivity` totals |
| `vue_vet_explain` | Rule id or opaque finding id (same payload as CLI `--explain`) |
| `vue_vet_explain_scope` | Same `ScopeExplain` JSON as CLI `--explain-scope` |
| `vue_vet_preview_safe_fixes` | Preview only; never write |

## Public API

| Item | Role |
| --- | --- |
| `run_stdio` | Read/write loop until stdin closes |
| `McpServer` | Per-path session slot + request dispatch |
| `read_message` / `write_message` | Framing |
| `list_tools` / `call_tool` / `TOOL_NAMES` | Tool surface |

## Session reuse

One `ProjectSession` per resolved tool path:

- `vue_vet_scan` / `vue_vet_preview_safe_fixes` **replace** the session (disk
  edits stay visible on the next call)
- `vue_vet_explain` / `vue_vet_explain_scope` **reuse** `current_snapshot`
  (finding explain requires a prior scan of the same path)

Tool failures return MCP tool results, not process-level errors.

## Layout

| Module | Role |
| --- | --- |
| `protocol.rs` | Framing + `McpServer` |
| `tools.rs` | Tool schemas and handlers |

## Related docs

- [architecture PCR](../../.agents/docs/architecture.md) (`vue_vet_mcp`)
- [JSON output](../../docs/json-output.md)
- [gotchas PCR](../../.agents/docs/gotchas.md) (MCP session reuse)
- [Workspace crates](../../docs/crates.md)

## License

MIT
