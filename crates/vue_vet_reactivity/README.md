# vue_vet_reactivity

Static **Vue reactivity dependency tracing** for [Vue Vet](https://github.com/alexzhang1030/vue-vet).

Given an [Oxc](https://oxc.rs/) semantic model (or a resolved module graph), this
crate builds a serializable `ReactivityGraph` (`vue_vet_core`) of Vue-resolved
bindings, tracking scopes, demand reads, guards, and inverted dependency edges —
without executing components, effects, or Proxies.

Lint rules are the first consumer. The graph is intended as a multi-consumer
library surface (project graph, cache, future LSP / codemod tools).

## Status

Early `0.x`. The fact schema is versioned
(`REACTIVITY_GRAPH_VERSION = 24` in `vue_vet_core`). See the repository PCR
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

let graph = trace_reactivity(&semantic, source, 0, ScriptKind::Setup);
assert!(!graph.bindings.is_empty());
assert!(!graph.scopes.is_empty());
```

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
`TraceModulesOptions { max_workers, ..Default::default() }` when the caller
needs an explicit concurrency bound (a dedicated Rayon pool is installed).
Long-lived session analysis sets `reuse_current_pool: true` so it shares the
outer `--threads` pool instead of nesting another. Vue Vet's Oxc adapter attaches
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
