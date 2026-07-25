# Reactivity tracer

`vue_vet_reactivity` is the Vue Vet-owned **static reactivity tracing library**.
Lint rules are the first consumer, not the capability ceiling. Crate-facing docs
live in [`crates/vue_vet_reactivity/README.md`](../../crates/vue_vet_reactivity/README.md);
this record holds product stance and completeness judgment.

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
- The CLI surfaces a **Reactivity** digest after the score line (and optional
  `--print-reactivity` detail) so a clean score is distinguishable from a
  no-op tracer. Machine consumers get the same totals via JSON `reactivity`.

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

Contract version: **`REACTIVITY_GRAPH_VERSION = 6`** (v5 + **`edge.to_id`** span-qualified
identities; bare `to` retained for rule matching).

| Axis | Status | Gap |
| --- | --- | --- |
| A1 Bindings | partial | Vue primitives, aliases, `#imports`, `defineModel`, `defineProps`, **`withDefaults(defineProps())`**, **`storeToRefs`**, **`useRoute`/`useRouter`**, **`unref`/`toValue` reads**, module seeds |
| A2 Scopes | partial | effects, **computed getter fn + `{ get, set }`**, watch (sources + callback outside), effectScope (`.run` requires provenance), dispose |
| A3 Reads | partial | `.value` / reactive members / bag.field / **sync Array HOF (+ sort)** / **String#replace/replaceAll** / **`Array.from` mapFn** / **`JSON.parse` reviver** / **watch ref sources `.value`** / **`unref`/`toValue`/`toValue(getter)`** |
| A4 Conditions | deep | if / early-exit / ternary / short-circuit / switch roles — do not deepen further yet |
| A5 Boundaries | partial | await, pauseTracking, deferred callbacks, watch jobs |
| A6 Modules | partial | composable shapes; instance bags; same-file + export const/default; **dual script: setup + `path#script` ordinary re-trace**; **provide/inject unique-key index (no App Tree)** |
| A7 Contract | improving | **v6**: `to_id = name@offset`; bare `to` still for consumers |
| Evidence | improving | Runtime oracle; exhaustive local fixtures; SFC E2E defineProps/instance/dual module sources |

### Deferred (honest — not “done”)

| Gap | Why deferred |
| --- | --- |
| Bare `watch(reactiveObj)` deep keys | Runtime tracks iterate + many property keys; property-less static dep invents identity |
| Full module-qualified `to` (module:name) | `to_id` is span-local; cross-module symbol IDs still optional |
| Further A4 control-flow depth | Already deep; wrong axis for recall |
| Whole-program JS soundness | Charter: under-approx Vue tracking, not full alias analysis |

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
4. **A1/A3 breadth** — `defineProps`, sync Array HOF, `storeToRefs`, `useRoute`,
   same-file + cross-module composable bags (**shipped core**); remaining breadth
   is long-tail APIs, not the primary axis.
5. **Stable edge identity** — `from` labels shipped (v4); symbol/module `to` still
   deferred (L5) until consumers need it.

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
| 2026-07-25 | A1 breadth: storeToRefs / useRoute + edge from-ids | pinia/vue-router allowlist; graph v4 edge `from` labels; more oracle cases |
| 2026-07-25 | Instance seed no top-level pollution | `const bag = useX()` seeds `composable_instances` only; shape fields are not top-level bindings |
| 2026-07-25 | Exhaustive local fixture reads | optional `expected.reads` exact effect set; pilot on systematic/01 + complex/01 |
| 2026-07-25 | Oracle breadth | `reactive-member`, `sync-reduce-hof`, `watch-effect-ref` |
| 2026-07-25 | Full corpus exhaustive reads | all 200 local fixtures pin exact effect read sets |
| 2026-07-25 | Oracle boundaries + HOF | `pause-tracking-window`, `sync-forEach-hof`, `sync-some-hof` |
| 2026-07-25 | Watch source dep keys | bare ref sources → `property: value`; bare reactive sources stay quiet |
| 2026-07-25 | Oracle watch + flatMap | `watch-source-{ref,array,getter}`, `sync-flatMap-hof` |
| 2026-07-25 | Graph v5 composable_instances | retain instance bags; template joins pure `bag.field` / `bag.field.value` |
| 2026-07-25 | SFC E2E | defineProps+template; seeded instance bag template join; every/find oracle |
| 2026-07-25 | Same-file local composables | `function useX` / `const useX = () =>` shapes; bag.field + destructure seeds; nested refs not published |
| 2026-07-25 | SFC offset shape resolution | `composable_return_shape` uses binding script_offset; same-file bag works in Vize/project SFC path |
| 2026-07-25 | Export const / default composables | module locals include const arrow/function shapes; default export named functions seed consumers |
| 2026-07-25 | Graph v6 `to_id` | edges carry `name@offset`; bare `to` kept for unused-binding etc. |
| 2026-07-25 | unref / toValue | sync tracking reads of ref-like first args |
| 2026-07-25 | Dual script modules | setup id + ordinary `path#script`; CLI applies both seeded graphs |
| 2026-07-25 | Oxlint-style parallelism | Rayon file facts + module phases + seed-aware rules; `--threads N` |
| 2026-07-25 | A1/A3: computed object get, sort HOF, withDefaults | `{ get, set }` getter body; Array#sort comparator; peel `withDefaults(defineProps())` |
| 2026-07-25 | A3: watch source array of getters | `watch([() => a.value, () => b.value])` and mixed `[ref, getter]` |
| 2026-07-25 | provide/inject unique-key seeds | project-wide provide index; exactly one known shape seeds inject; multi-provide quiet; defaults allowed |
| 2026-07-25 | inject key identity + bag provide | Imported `(specifier,export)` vs Local `def_span`; provide(composable bag) seeds inject instances |
| 2026-07-25 | toValue(getter) + provide(useX()) | Getter arg tracks like sync HOF; direct composable call provide seeds inject bag |
| 2026-07-25 | String#replace/replaceAll HOF | Replacer callback tracks nested reactive reads (sync, like Array HOF) |
| 2026-07-25 | Array.from mapFn + JSON.parse reviver | Well-known static sync callbacks only (`Array`/`JSON` receiver) |
| 2026-07-25 | under-approx fixes: provide span + HOF arg index | provide(useX()) resolves def span; replace/from/parse callback only at arg 1 |
| 2026-07-25 | crates.io library packaging | crate README + rustdoc; publish `vue_vet_core` then `vue_vet_reactivity` to reserve names; fixtures/oracle git-only |
