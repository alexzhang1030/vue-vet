# vue_vet_mcp

Thin **MCP adapter** over [`vue_vet_session`](../vue_vet_session/README.md).

Stdio JSON-RPC (Content-Length framing) exposing scan, explain, explain-scope,
and safe-fix **preview** tools. The live server keeps one `ProjectSession` per
resolved tool path: scan / preview replace it; explain / explain-scope reuse the
last committed snapshot. Apply remains CLI / LSP — MCP never writes.

## Status

Workspace-internal (`publish = false`). Entered via `vue-vet --mcp`
(`run_stdio`).

## Tools

| Tool | Role |
| --- | --- |
| `vue_vet_scan` | Analyze path; JSON includes CLI-shaped `reactivity` totals |
| `vue_vet_explain` | Rule or finding documentation |
| `vue_vet_explain_scope` | Same `ScopeExplain` JSON as CLI `--explain-scope` |
| `vue_vet_safe_fix_preview` | Preview only; never apply |

## Related docs

- [architecture PCR](../../.agents/docs/architecture.md) (`vue_vet_mcp`)
- [JSON output](../../docs/json-output.md)
- [Workspace crates](../../docs/crates.md)

## License

MIT
