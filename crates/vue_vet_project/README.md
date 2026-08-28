# vue_vet_project

Deterministic **Vue / Nuxt project graph** for
[Vue Vet](https://github.com/alexzhang1030/vue-vet).

Owns the versioned graph pipeline: conventions → structural links → enrichment
→ reactivity trace handoff → template/prop layers → project diagnostics.
`lib.rs` is a thin façade; orchestration lives in `pipeline`.

Does **not** own file-rule packs, CLI/session I/O, or a dynamic JS plugin host.
Enrichment passes must not `report` diagnostics — only diagnostic `Rule`s do.

## Status

Workspace-internal (`publish = false`). Product builds auto-load
`vue_vet_plugins` via `default_trace_modules_options()` /
`ensure_default_plugins_mut`.

## Versions

| Constant | Value | Role |
| --- | --- | --- |
| `CONVENTIONS_VERSION` | `14` | Nuxt / Vite map + resolver convention identity |
| `OXC_RESOLVER_VERSION` | `"11.21.0"` | Pinned resolver (cache key field) |
| `EXTERNAL_COMPANION_MAX_BYTES` | 1 MiB | Cap for companion `.js` body loads |
| `PROJECT_RULE_IDS` | `unresolved-import`, `unused-component` | Project diagnostics |

## Pipeline

```text
context       ConventionsLoad → ProjectContext (epochs, maps)
structural    StructuralLink (import / component edges;
              NuxtImportsSeedPass for bare auto-imports)
passes        ExternalSummaryLoadPass (+ ProvisionalFactoryMergePass per load)
pipeline      SeedPlan / Trace via vue_vet_reactivity → ProjectGraph
layers        template joins + prop-flow
rules         unresolved-import / unused-component
```

Enrichment is compile-time named `::run` steps over Vue Vet IR (`ENRICHMENT_STEPS`
checklist) — not Oxc/SWC AST Traverse, not `dlopen` / npm plugins.

No Vite/Nuxt **config execution**. Maps come from `.nuxt` dts, tsconfig paths,
package exports, and similar static inputs. Dual-script SFCs use `path#script`
for the ordinary block.

## Public API (façade)

| Area | Items |
| --- | --- |
| Build | `build_project_graph`, `build_project_graph_with_options`, `build_project_graph_incremental_with_options` |
| Context | `ProjectContext`, `ContextEpochs`, `ContextChangeKind`, `project_context_from_inputs` |
| Model | `ProjectGraph`, `ProjectFile`, `GraphNode` / `GraphEdge`, `NodeKind` / `EdgeKind`, `ReactivityIssue` |
| Passes | `ENRICHMENT_STEPS`, `ExternalSummaryLoadPass`, `NuxtImportsSeedPass`, `ProvisionalFactoryMergePass` |
| Resolve | `normalize_project_root`, `resolver_config_inputs`, `OXC_RESOLVER_VERSION` |
| State | `ProjectGraphState`, `ProjectGraphStats` |

## Constraints

- Sorted outputs; quiet under-approximation.
- Context epoch bumps (package / lockfile / tsconfig / Nuxt / membership) must
  not be equated with re-parse — see architecture dirty-set notes.
- Diff mode must not drop project findings caused by a remote change
  (`filter_diff` in `vue_vet_cache` keeps `category == "project"`).

## Related docs

- [Project graph](../../docs/project-graph.md)
- [architecture PCR](../../.agents/docs/architecture.md) (`vue_vet_project` pipeline)
- [ADR 0001](../../docs/adr/0001-analysis-stack.md)
- [Workspace crates](../../docs/crates.md)

## License

MIT
