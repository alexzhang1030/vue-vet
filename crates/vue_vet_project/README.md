# vue_vet_project

Deterministic **Vue / Nuxt project graph** for
[Vue Vet](https://github.com/alexzhang1030/vue-vet).

Builds versioned nodes and edges (imports, components, auto-imports) from
serializable Vize/Oxc facts, runs compile-time enrichment passes, hands modules
to `vue_vet_reactivity` for seed linking, then attaches template / prop layers
and project-level diagnostics. `lib.rs` is a thin façade over an explicit
pipeline.

## Status

Workspace-internal (`publish = false`). Product scans auto-load
`vue_vet_plugins` defaults via `default_trace_modules_options()`.

## Pipeline

```text
context          ConventionsLoad → ProjectContext
structural       StructuralLink (import / component edges)
passes           enrichment (Nuxt seeds, external summaries, provisional Factory)
pipeline         Trace handoff + ProjectGraph assembly
layers           template joins + prop-flow
rules            unresolved-import / unused-component
model / state    DTOs + retained incremental partitions
resolve / conventions   oxc_resolver + Nuxt maps
```

Enrichment passes are named Rust `::run` steps over Vue Vet IR — not AST
Traverse and not a dynamic JS plugin host.

## Related docs

- [Project graph](../../docs/project-graph.md)
- [architecture PCR](../../.agents/docs/architecture.md) (`vue_vet_project` pipeline)
- [Workspace crates](../../docs/crates.md)

## License

MIT
