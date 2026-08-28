# vue_vet_session

Long-lived **project analysis session** shared by CLI, LSP, and agent surfaces.

Owns configuration loading, content-addressed cache orchestration, workspace
discovery, unsaved buffer overlays, full and incremental scans, rule / finding /
scope explain, progress events, and workspace path containment.

Protocol adapters (clap, LSP, MCP) stay **outside**. Disk apply of safe fixes
stays in the CLI — this crate attaches edit candidates to findings and never
mutates files itself.

## Status

Workspace-internal (`publish = false`). Dependents: `vue-vet`, `vue_vet_lsp`,
`vue_vet_mcp`.

## Public API

| Item | Role |
| --- | --- |
| `ProjectSession::{open, analyze, analyze_fresh, analyze_with_overlays, …}` | Session handle |
| `apply_changes` / `analyze_affected` / `analyze_affected_product` | Incremental |
| `SessionOptions` | `root`, `config_path`, `cache_dir`, `no_cache`, `threads` |
| `AnalysisSnapshot` | Arc summary/graph/coverage/issues/work |
| `ChangeSet` / `ChangeImpact` / `DirtyPlan` / `ScanWorkCounters` | Locality |
| `AnalysisProduct` | `DiagnosticsOnly` / `DiagnosticsAndNavigation` / `FullReport` |
| `Explained::{Rule, Finding}` | Explain outcomes |
| `explain` / `explain_rule` / `explain_finding` / `explain_scope` | Docs + scope |
| `ProgressEvent` / `ProgressReporter` | stderr-stage streaming hooks |
| `resolve_under_root` / `discover_workspace_boundary` / `scan_directory` | Paths |

## Layout

| Module | Why it exists |
| --- | --- |
| `session.rs` | Lock domain, revision, commit / cancel |
| `pipeline/` (+ `analyze`) | discovery → facts → project → rules → finalize |
| `types.rs` | Snapshot / options / issues |
| `locality.rs` | DirtyPlan / work counters / product levels |
| `config.rs` | Discover + validate `vue-vet.toml` |
| `registry.rs` | File-rule + practice metadata / known ids |
| `explain.rs` | Rule / finding / scope explain |
| `discovery.rs` / `package_index.rs` / `scan.rs` | Input snapshot + packages |
| `diagnostics.rs` | DiagnosticFinalizer |
| `invalidation.rs` / `progress.rs` / `path.rs` | Cache keys, progress, containment |

## Constraints

- One retained `WorkspaceInputSnapshot` per workspace revision; cache hit and
  miss share the same bytes (no second walk/read).
- Overlay analysis bypasses the content-addressed cache.
- Atomic publication: revision + snapshot + commit under one session lock;
  stale latest-wins work is cancelled between phases.
- Shared Rayon pool is created lazily on the first real scan; tracer gets
  `reuse_current_pool: true`.
- Dirty parse is real; structural / module partitions may still rebuild broadly
  (Post-#107 locality gap) — prove cost with `ScanWorkCounters`.
- Context epoch ≠ re-parse.
- File/module failures become scoped `AnalysisIssue`; fatal config still fails
  the request.
- Explain-scope queries: binding / `module:binding` / `@offset` (start, else
  tightest covering scope).

## Related docs

- [architecture PCR](../../.agents/docs/architecture.md) (`vue_vet_session`, dirty-set)
- [JSON output](../../docs/json-output.md)
- [Cache / baselines / diff](../../docs/cache-baseline-diff.md)
- [Edit model](../../docs/edit-model.md)
- [Workspace crates](../../docs/crates.md)

## License

MIT
