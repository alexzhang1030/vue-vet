# vue_vet_lsp

Thin **stdio LSP** adapter over [`vue_vet_session`](../vue_vet_session/README.md).

Publishes diagnostics for on-disk files and unsaved buffer overlays with the
same opaque finding ids as CLI JSON (`data` field). Offers explicitly safe
quick-fix code actions as **versioned workspace edits** (client applies; server
never writes). Hover answers “would Vue re-run?” via `--explain-scope`
`file:@offset` markdown.

Does **not** reimplement analysis or write files.

## Status

Workspace-internal (`publish = false`). Entry: `vue-vet --lsp` → `run_stdio()`.

## Public API

| Item | Role |
| --- | --- |
| `run_stdio` | Tokio + `tower-lsp` stdin/stdout server |
| `Backend` | LSP handlers over a `ProjectSession` |
| `is_current_generation` | Latest-wins gate helper |
| `to_lsp_diagnostic` / `span_to_range` / `position_to_byte` | Convert |
| `explain_scope_query` / `hover_from_scope_explains` | Hover |
| `safe_code_actions` / `SafeCodeActionRequest` | Quick fix |

## Layout

| Module | Role |
| --- | --- |
| `server.rs` | LSP lifecycle, debounce, publish |
| `convert.rs` | Span / diagnostic / hover / code-action mapping |

## Behavior notes

- FULL document sync overlays; overlay changes advance the workspace revision
  in the same critical section as the retained input snapshot.
- 50 ms debounce + single latest-wins task; stale work cancels between pipeline
  phases and its commit is rejected under the session lock.
- Diagnostics publish may use `AnalysisProduct::DiagnosticsOnly`; the committed
  snapshot still keeps enough graph state for hover.
- Finding id in LSP `data` matches JSON `diagnostics[].id`.

The thin VS Code extension ([`editors/vscode`](../../editors/vscode/README.md))
can shell out to CLI JSON without this LSP; full editor LSP is this crate.

## Related docs

- [architecture PCR](../../.agents/docs/architecture.md) (`vue_vet_lsp`)
- [JSON output](../../docs/json-output.md) (`--explain-scope`)
- [Workspace crates](../../docs/crates.md)

## License

MIT
