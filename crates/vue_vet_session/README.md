# vue_vet_session

Long-lived **project analysis session** shared by CLI, LSP, and agent surfaces.

Owns configuration loading, content-addressed cache, workspace discovery,
unsaved buffer overlays, full and incremental scans, rule / finding / scope
explain, and workspace path containment. Protocol adapters (clap, LSP, MCP)
stay outside this crate.

## Status

Workspace-internal (`publish = false`). Product entry: `ProjectSession` with
`SessionOptions`, `apply_changes` / `analyze_affected`, and
`ChangeImpact` / `DirtyPlan` work counters.

## Layout

```text
session.rs     ProjectSession orchestration
pipeline/      discovery → facts → project → rules → finalize
types.rs       AnalysisSnapshot / SessionError / options
locality.rs    DirtyPlan / ScanWorkCounters
config.rs      vue-vet.toml discovery + rule-id validation
explain.rs     rule / finding / scope explain
```

## Related docs

- [architecture PCR](../../.agents/docs/architecture.md) (`vue_vet_session`)
- [JSON output](../../docs/json-output.md)
- [Cache / baselines / diff](../../docs/cache-baseline-diff.md)
- [Workspace crates](../../docs/crates.md)

## License

MIT
