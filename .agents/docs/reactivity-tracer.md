# Reactivity tracer

`vue-vet-reactivity` is the Vue Vet-owned **static reactivity tracing library**.
Lint rules are the first consumer, not the capability ceiling.

Related: [architecture](./architecture.md), [gotchas](./gotchas.md),
[literature matrix](./research/reactivity-tracer-literature.md),
[root roadmap](../../ROADMAP.md).

## Product stance

- Approximate Vue's **synchronous tracking semantics** with static facts.
  Do not execute components, effects, or Proxies for product analysis.
- Prefer **under-approximation + quiet failure** over inventing edges.
- Keep Vue Vet-owned serializable contracts independent of Oxc and Vize types.
- Grow the graph so multiple consumers can share it: rules, project graph,
  cache, future LSP/codemod surfaces.

## What “complete” means

Completeness is coverage of Vue reactivity semantics, not whole-program
JavaScript soundness.

| Axis | Question the lib must answer |
| --- | --- |
| A0 Semantics | Which Vue tracking rules are in scope, and where do we stop? |
| A1 Bindings | Which locals are reactive, with what kind and identity? |
| A2 Scopes | Which code regions perform dependency tracking? |
| A3 Reads | Which reactive reads occur in a scope, with what path/property? |
| A4 Conditions | Under which control conditions is a read demandable? |
| A5 Boundaries | Where does synchronous tracking end (await, nested callback, …)? |
| A6 Modules | How do composables and exports seed consumer bindings? |
| A7 Contract | Is the graph versioned, deterministic, and multi-consumer stable? |

## Current baseline (honest)

Contract version: **`REACTIVITY_GRAPH_VERSION = 3`** (scopes, writes, edges,
`template_reads`, effects projection).

| Axis | Status | Gap |
| --- | --- | --- |
| A1 Bindings | partial | Vue primitives, aliases, `#imports`, `defineModel`, **`defineProps` → reactive object**, module seeds. Still missing storeToRefs / route sources |
| A2 Scopes | partial | effects, computed, watch, effectScope (`.run` requires provenance), dispose |
| A3 Reads | partial | direct `.value` / reactive members / bag.field.value / **sync Array HOF callbacks** (filter/map/…). Still missing some HOF surfaces |
| A4 Conditions | deep | if / early-exit / ternary / short-circuit / switch roles — **over-invested relative to A1/A3** |
| A5 Boundaries | partial | await, pauseTracking, deferred callbacks, watch jobs |
| A6 Modules | partial | composable shapes, parametric `toRef`, SFC module identity, seed→rules |
| A7 Contract | shipped | versioned graph; edge IDs still fragile (`callee@offset`, bare names) |
| Evidence | improving | Runtime oracle (`oracle/expected`, `just oracle`) asserts **tracer ⊆ runtime** and gates **≥99% recall** on committed cases. 280 corpus remains a syntax matrix, not completeness proof |

### Charter invariants (must not regress)

1. **Under-approx:** invented edges are bugs; missing edges are acceptable quiet failure.
2. **No runtime execution** as the product engine (runtime may be an **oracle** for tests).
3. **Symbol identity** for cross-module linking; bare names are not enough (gotchas).

## Reorientation (2026-07-25)

Waves 1–8 deepened **A4 / template join / module plumbing**. That is useful
infrastructure, but real components still under-report because **A1/A3 breadth**
was never expanded.

Hard failures (oracle + unit):

| Case | Status |
| --- | --- |
| `defineProps` → `props.count` in `computed` | **fixed** (defineProps → reactive binding) |
| `list.value.filter(x => x.includes(query.value))` | **fixed** (sync Array HOF callbacks) |
| `runner.run(() => count.value)` when `runner` is not `effectScope` | **fixed** (provenance required) |

**Correct next order:**

1. **Runtime oracle** — **shipped skeleton**: Vue `onTrack` harness + committed
   `oracle/expected/*.json`; Rust asserts `tracer ⊆ runtime` and ≥99% recall on
   those cases (`just oracle` / `just oracle-refresh`).
2. **Exhaustive fixture asserts** — full read/guard/edge sets, not only
   “expected binding found”; drop integer-padding corpus gates as completeness proof.
3. **Kill inventions** — `.run` requires `effectScope` provenance (**shipped**);
   review parametric pass-through and instance seed injection.
4. **A1/A3 breadth** — `defineProps` + sync Array HOF (**started**); still need
   `storeToRefs`, `useRoute`, more HOF/call shapes.
5. **Stable edge identity** — symbol/module-qualified `from`/`to` before more
   consumers depend on the graph.

Do **not** deepen A4 further until oracle coverage and A1 breadth keep growing.

### Prior art (verified)

There is **no official Vue “reactivity analysis plugin”** that builds a static
dependency graph. Related pieces:

| Artifact | What it is | Overlap |
| --- | --- | --- |
| `eslint-plugin-vue` reactivity-loss rules | shallow AST patterns (`no-setup-props-reactivity-loss`, …); not a graph | different rules; no edge set |
| Vapor Mode compiler | static deps for codegen, not a public IR | same *problem*, different product |
| Vue DevTools | runtime graph | oracle ground truth, not lint |

Differentiation still holds: a **serializable static reactivity graph library**
is the gap. Vue 3.6 / alien-signals rewrites raise the value of a runtime oracle
as both precision ruler and version-compat net (`pauseTracking` etc. must stay
capability-gated).

## Completeness ladder (revised)

| Level | Focus | Exit |
| --- | --- | --- |
| L0 Charter | under-approx, static-only, quiet failure | this file + gotchas |
| L1 Scopes | tracking regions without invention | no false effectScope; known APIs only |
| L2 A1/A3 breadth | props, sync HOF, common composables | oracle recall improves on real SFCs |
| L3 Boundaries | await / pause / deferred | version-gated Vue APIs |
| L4 Modules | seeds without top-level pollution | symbol identity across files |
| L5 Contract | stable edge IDs + version | multi-consumer safe |
| L6 Template join | Vize surfaces + Oxc free ids | shipped infrastructure; not a substitute for A1 |

## Shipped infrastructure (condensed)

Landed as evolution waves (do not re-litigate; do not treat as completeness):

- Scope IR, guards, after-await / outside-tracking, prefer-computed / unused-binding rules
- Template expression facts, Oxc free-ids, v-for/slot alias scopes
- SFC `ModuleSource`, seed spans, CLI two-phase seed→rules
- Template joins on module graphs

Details live in git history on `feat/reactivity-tracer-evolution` rather than a
growing prose ledger.

## Decision log

| Date | Decision | Notes |
| --- | --- | --- |
| 2026-07-24 | Lib-first completeness over rule-only ROI | Tracer is an ecosystem library; lint is first consumer |
| 2026-07-24 | Static approximation only | Runtime is the semantic reference, never the product execution mode |
| 2026-07-24 | Under-approx + quiet failure remains default | Completeness does not mean guessing |
| 2026-07-24–25 | Waves 1–8: A4 depth, template join, module plumbing | Useful infra; **wrong primary axis for “complete”** |
| 2026-07-25 | Reorient to A1/A3 + runtime oracle | Guards only matter when edges exist; 280 corpus ≠ recall |
| 2026-07-25 | No official Vue reactivity-analysis plugin | Prior art is shallow ESLint rules + Vapor codegen + DevTools runtime |
| 2026-07-25 | Runtime oracle skeleton + A1 fixes | onTrack expected JSON; defineProps; sync filter/map HOF; effectScope.run provenance |
