# vue_vet_lsp

Thin **LSP adapter** over [`vue_vet_session`](../vue_vet_session/README.md).

Publishes diagnostics for on-disk files and unsaved buffer overlays with the
same opaque finding ids as CLI JSON, offers explicitly safe quick-fix code
actions as versioned workspace edits (client applies; server never writes), and
answers hover with the same `ScopeExplain` markdown as CLI `--explain-scope`
(`file:@offset`). A debounced latest-wins queue cancels stale session revisions
between analysis phases.

## Status

Workspace-internal (`publish = false`). Entered via `vue-vet --lsp`
(`run_stdio`).

## Related docs

- [architecture PCR](../../.agents/docs/architecture.md) (`vue_vet_lsp`)
- [JSON output](../../docs/json-output.md) (`--explain-scope`)
- [VS Code host](../../editors/vscode/README.md)
- [Workspace crates](../../docs/crates.md)

## License

MIT
