# vue_vet_reactivity

Static **Vue reactivity dependency tracing** for [Vue Vet](https://github.com/alexzhang1030/vue-vet).

Given an [Oxc](https://oxc.rs/) semantic model (or a resolved module graph), this
crate builds a serializable `ReactivityGraph` (`vue_vet_core`) of Vue-resolved
bindings, tracking scopes, demand reads, guards, and inverted dependency edges —
without executing components, effects, or Proxies.

Lint rules are the first consumer. The graph is intended as a multi-consumer
library surface (project graph, cache, CLI `--explain-scope`, MCP
`vue_vet_explain_scope`, LSP hover, future codemod tools).

### Explain a tracking scope

```rust
use vue_vet_reactivity::{explain_tracking_scope, select_tracking_scopes};

// After `trace_reactivity` / `trace_modules`:
for scope in select_tracking_scopes("App.vue", &graph, "doubled") {
  let explain = explain_tracking_scope("App.vue", scope);
  // explain.summary — one-line "would Vue re-run?"
  // explain.tracks / does_not_track / uncertain
}
```

CLI: `vue-vet path --explain-scope doubled` (or `App.vue:doubled`, `@offset`).
`@offset` matches a span start first, then the tightest covering scope (same as
finding `--explain`). `callee@offset` stays exact-start.
MCP: tool `vue_vet_explain_scope` with the same query (JSON object or array).
LSP: `vue-vet --lsp` hover at a caret inside a tracking scope.

## Status

Early `0.x`. The fact schema is versioned
(`REACTIVITY_GRAPH_VERSION = 34` in `vue_vet_core`). See the repository PCR
([reactivity tracer](https://github.com/alexzhang1030/vue-vet/blob/main/.agents/docs/reactivity-tracer.md))
for the ExportState lattice and axis checklist. In-scope design axes A1–A7
and Evidence are **complete** — complete means the in-scope checklists, not
whole-program JS soundness. Contract refinements still bump the graph version.
Treat the Rust API as evolving until Vue Vet hits a stable release. Prefer
**under-approximation**: missing edges are quiet failure; invented edges are
bugs.

## Install

```toml
[dependencies]
vue_vet_reactivity = "0.1"
vue_vet_core = "0.1"
# Optional: Nuxt / vue-i18n named API bags (product CLI loads these automatically)
# vue_vet_plugins = "0.1"
```

You also need a pinned Oxc semantic stack compatible with this crate's
`oxc_*` dependencies (see `Cargo.toml`).

## Quick start (single script)

```rust
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use vue_vet_core::ScriptKind;
use vue_vet_reactivity::trace_reactivity;

let source = r#"
import { ref, computed } from 'vue'
const count = ref(0)
const doubled = computed(() => count.value * 2)
"#;

let allocator = Allocator::default();
let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
let semantic = SemanticBuilder::new()
  .with_check_syntax_error(true)
  .build(&parsed.program)
  .semantic;

// Empty plugin catalog: Vue primitives only.
let graph = trace_reactivity(&semantic, source, 0, ScriptKind::Setup);
assert!(!graph.bindings.is_empty());
assert!(!graph.scopes.is_empty());
```

### Ecosystem plugins (Nuxt / vue-i18n)

This crate does **not** hardcode Nuxt or vue-i18n surfaces. Pass a
`NamedApiBag` catalog via `TraceConfig`:

```rust
use vue_vet_plugins::default_trace_config;
use vue_vet_reactivity::trace_reactivity_with_config;

let graph = trace_reactivity_with_config(
  &semantic,
  source,
  0,
  ScriptKind::Setup,
  &default_trace_config(),
);
```

Multi-module: set `TraceModulesOptions::named_api_bags` (or use
`vue_vet_plugins::default_trace_modules_options()`).

The Vue Vet CLI / session / Oxc adapter **auto-load** default plugins so product
scans always include the catalog. See [vue_vet_plugins](../vue_vet_plugins/README.md).

Spans are byte offsets into the original file. For Vue SFC script blocks, pass
the full SFC text and the script body's byte offset so line/column map back to
the `.vue` file.

## Module graph

For cross-file composables, barrels, and unique-key `provide` / `inject` seeds,
build `ModuleSource` values (standalone or SFC script) plus already-resolved
`ModuleLink` edges, then call `trace_modules`:

```rust
use vue_vet_core::ScriptKind;
use vue_vet_reactivity::{ModuleLink, ModuleSource, trace_modules};

let modules = vec![
  ModuleSource::standalone(
    "producer.ts",
    "import { ref } from 'vue'\nexport function useCounter() {\n  return { count: ref(0) }\n}\n",
    "ts",
    ScriptKind::Ordinary,
  ),
  ModuleSource::standalone(
    "consumer.ts",
    "import { computed } from 'vue'\nimport { useCounter } from './producer'\nconst { count } = useCounter()\nconst label = computed(() => count.value)\n",
    "ts",
    ScriptKind::Ordinary,
  ),
];
let links = vec![ModuleLink {
  from: "consumer.ts".into(),
  specifier: "./producer".into(),
  to: "producer.ts".into(),
}];

let graphs = trace_modules(&modules, &links).expect("trace");
assert_eq!(graphs.len(), 2);
```

Link resolution is the caller's job (Vue Vet's project graph supplies it). This
crate does not open the filesystem or resolve bare specifiers.
Use `trace_modules_with_options` and
`TraceModulesOptions { max_workers, named_api_bags, ..Default::default() }` when
the caller needs an explicit concurrency bound and/or ecosystem bags (a
dedicated Rayon pool is installed when `reuse_current_pool` is false).
Long-lived session analysis sets `reuse_current_pool: true` so it shares the
outer `--threads` pool instead of nesting another, and installs default plugins
via `vue_vet_plugins`. Vue Vet's Oxc adapter attaches
a `ModuleSummary` (module semantic IR) from its file parse, so unseeded modules
are not parsed again; only consumers that receive cross-module seeds reparse for
symbol materialization. Attach summaries with
`ModuleSource::with_module_summary`. Long-lived callers should retain
`ModuleTraceState` and call `trace_modules_incremental_with_options`: unchanged
source + seed-plan pairs reuse their final graph, and the returned report scopes
errors per module while preserving healthy cross-module results.

## What the graph contains

| Field | Meaning |
| --- | --- |
| `bindings` | Locals recognized as Vue reactive (`ref`, `reactive`, `computed`, props, …) |
| `scopes` | Tracking regions (`watchEffect*`, `computed`, `watch` sources, `effectScope`, …) with classified reads and guards |
| `effects` | Legacy projection of effect-family scopes |
| `edges` | Inverted deps (`from` → `to` / `to_id`) for computed, effect, and template joins |
| `composable_instances` | `const bag = useX()` shapes for `bag.field` resolution |

Design axes and honesty bounds live in the repository PCR:
[reactivity tracer](https://github.com/alexzhang1030/vue-vet/blob/main/.agents/docs/reactivity-tracer.md).

## Charter

1. **Static only** — runtime Vue is the semantic reference / test oracle, not the product engine.
2. **Under-approx** — do not invent dependency edges.
3. **Stable Vue Vet types** — Oxc AST nodes never appear in the returned graph.
4. **Deterministic** — sorted facts; no hash-map iteration order in output.

## Evidence

In-tree tests include exhaustive local fixtures and a Vue `onTrack` runtime
oracle (`oracle/`, refreshed with `just oracle-refresh`). Gate: `just oracle`
asserts `tracer ⊆ runtime` and ≥99% recall on the **committed representative
cases** (not every SFC in the wild). Deep `watch(reactive)` uses static
`property: "*"`. Cross-file prop flow is covered by unit/project fixtures
(static join; not an `onTrack` pair). Published crates omit those fixtures;
clone the repository to run them.

## License

MIT
