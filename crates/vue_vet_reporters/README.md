# vue_vet_reporters

Deterministic **text and machine-readable reporters** for
[Vue Vet](https://github.com/alexzhang1030/vue-vet).

Consumes Vue Vet-owned `ScanSummary` plus an explicit `ReportContext`. Does
**not** own session state, run analysis, mutate files, or decide CLI exit codes.
Explain **domain models** live in `vue_vet_core` / `vue_vet_session`; this crate
renders them.

## Status

Workspace-internal (`publish = false`). Shared by CLI, MCP scan JSON, LSP hover
markdown helpers, and explain surfaces so diagnostic identity stays consistent.

## Formats

| `ReportFormat` | Contract |
| --- | --- |
| `Text` | Human diagnostics + score footer; optional ANSI via `ReportContext.color` |
| `Json` | `JSON_SCHEMA_VERSION = 1` wire contract |
| `Sarif` | SARIF 2.1.0 |
| `Github` | Escaped GitHub Actions workflow commands |

`ReportMode`: `Full` / `Baseline` / `Diff`. `ReportFramework`: `Vue` / `Nuxt`.

Renderers return content **without** a trailing newline so each surface chooses
framing. Text snapshots are byte-for-byte gates (color off); JSON snapshots are
versioned wire gates.

## Public API

| Area | Items |
| --- | --- |
| Dispatch | `render`, `ReportContext`, `ReportFormat`, `ReportMode`, `ReportFramework` |
| Errors | `render_error`, `report_diagnostic_id` |
| Explain | `explain_rule` / `explain_finding` / `finding_explain_with_tracking`, `render_*_explain_*`, `documentation_path`, `looks_like_finding_id` |
| Scope | `render_scope_explain_{text,markdown,json}` (+ plural variants) |
| Reactivity | `ReactivityDigest`, `binding_detail`, `scope_detail*`, `render_reactivity_detail` / `_footer` |
| Nav | `binding_nav_from_details`, `component_nav_from_edges` |
| Humanize | `humanize_binding` / `_scope` / `_edge` / … |
| Re-exports | `FindingExplain`, `RuleExplain`, `ScopeExplain`, … from `vue_vet_core` |

`component_nav` is structural `uses` / `used_by` only — **not** prop dataflow
(see reactivity tracer A6 / `prop_flow`).

## Constraints

- Incomplete scans must expose `complete` and `skipped_check_reasons`; empty
  findings ≠ clean when `complete` is false.
- JSON paths use discovery-normalized `FileId` strings (no loose suffix match).
- Color applies only to interactive text; JSON / SARIF / GitHub stay uncolored.

## Related docs

- [JSON output](../../docs/json-output.md)
- [SARIF / GitHub](../../docs/sarif-github.md)
- [architecture PCR](../../.agents/docs/architecture.md) (reporting)
- [Workspace crates](../../docs/crates.md)

## License

MIT
