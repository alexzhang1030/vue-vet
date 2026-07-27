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
- The CLI surfaces a **Reactivity** digest after the score line (optional
  `--print-reactivity` text detail, or `--reactivity-tui` interactive browser)
  so a clean score is distinguishable from a no-op tracer. Machine consumers get
  the same totals via JSON `reactivity`; `--print-reactivity` also emits
  structured `*_details` spans for editor hosts. The TUI ranks modules
  busiest-first, hides empty modules by default, supports click/wheel mouse
  input, and requires an interactive TTY. `editors/vscode` is a thin CLI
  consumer (not LSP).

## What “complete” means

Completeness is coverage of **in-scope** Vue synchronous tracking semantics for
the product charter — not whole-program JavaScript soundness. An axis is
`complete` when its checklist below is green under under-approx + oracle gates.
Long-tail APIs and alias analysis stay **out of scope** and do not block
complete.

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

## Current baseline

Contract version: **`REACTIVITY_GRAPH_VERSION = 8`** (v7 + **module-qualified
`to_id`** `{module}:{name}@{offset}`; v7 `property` / `to_path` retained; bare
`to` kept for rule matching).

| Axis | Status | Covered (in-scope) | Remaining |
| --- | --- | --- | --- |
| A1 Bindings | complete | Vue primitives, aliases, `#imports`, bare Nuxt/auto-import allowlist, `defineModel`, `defineProps`, `withDefaults(defineProps())`, `storeToRefs`, `useRoute`/`useRouter`, `unref`/`toValue`, module seeds | — (long-tail APIs → out of scope) |
| A2 Scopes | complete | effects, computed getter/`{ get, set }`, watch sources + callback outside, effectScope `.run` + provenance, dispose | — |
| A3 Reads | complete | `.value` / members / bag.field / sync Array·String·`Array.from`·`JSON.parse` HOF / watch ref `.value` / `unref`·`toValue` / bare `watch(reactive)` deep root `*` | — |
| A4 Conditions | complete | if / early-exit / ternary / short-circuit / switch roles | — (no further depth) |
| A5 Boundaries | complete | after-await; pause/enable/resetTracking windows; nested `then`/`nextTick` outside; watch callback outside | — |
| A6 Modules | complete | composable shapes; instance bags; dual script; provide/inject unique-key; static `:prop` → child `props` Prop edges | — |
| A7 Contract | complete | v8 module-qualified `to_id`; v7 `property` / `to_path`; deterministic sort | — |
| Evidence | complete | Runtime oracle (≥99% recall on committed cases); deep-watch `*`; exhaustive local reads; key SFC E2E | — (prop flow is static unit/project; not an `onTrack` pair) |

### In-scope complete checklists

| Axis | Checklist (all required for `complete`) |
| --- | --- |
| A1 | ✅ Allowlist primitives + macros + pinia/router + auto-import + module seeds; local lookalikes quiet; unit/oracle cover |
| A2 | ✅ effect / computed / watch / effectScope.run(+provenance) / dispose scopes; no invented effectScope |
| A3 | ✅ Member/HOF/unref·toValue reads; watch ref `.value`; **deep root `*` for bare `watch(reactive)`** (not per-key invention) |
| A4 | ✅ Existing guard roles; no further control-flow deepening |
| A5 | ✅ After-await classification; pause/enable/resetTracking windows; nested callback outside-tracking; watch callback outside |
| A6 | ✅ Composable/instance/dual-script/provide-inject; **static `:prop` → child props bag edges** |
| A7 | ✅ Versioned graph; deterministic sort; `property`/`to_path`; **`{module}:{name}@{offset}` `to_id`** |
| Evidence | ✅ `just oracle` ≥99% recall on committed cases; exhaustive local reads; key SFC E2E |

### In-scope remaining (this epic)

None — deep-watch `*`, v8 `to_id`, and static prop flow shipped. Further breadth is out of scope below.

### Out of scope / A0 stop (never blocks complete)

| Gap | Why |
| --- | --- |
| Further A4 control-flow depth | Already deep; wrong axis for recall |
| Whole-program JS soundness / full alias analysis | Charter: under-approx Vue tracking only |
| App Tree provide/inject | Unique-key index is the in-scope model |
| Long-tail reactivity APIs beyond the allowlist | Quiet failure; expand allowlist only with oracle evidence |
| Inventing nested keys for deep `watch(reactive)` | Violates under-approx; deep root `*` is the contract |

### Charter invariants (must not regress)

1. **Under-approx:** invented *concrete* property keys are bugs; missing edges are
   acceptable quiet failure. Deep-watch root `property: "*"` is an explicit,
   oracle-aligned sentinel — not a guess at nested fields.
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
5. **Stable edge identity** — `from` labels shipped (v4); module-qualified `to_id`
   is the v8 contract (this epic).

Do **not** deepen A4 further. Expand A1/A3 only with oracle-backed allowlist growth.

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
| 2026-07-25 | Watch source dep keys | bare ref sources → `property: value`; bare reactive later → deep root `*` (2026-07-27) |
| 2026-07-27 | Literal axis complete epic | Product complete = in-scope checklists; deep-watch `*`, v8 module `to_id`, prop flow; whole-program JS stays A0 stop |
| 2026-07-27 | Axes A1–A7 + Evidence → `complete` | In-scope checklists green; deep root `*`; module-qualified `to_id`; Prop edges; A5 pause/enable/reset + nextTick fixtures; oracle gate = representative recall, not all SFCs |
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
| 2026-07-26 | Bare Nuxt/auto-import Vue APIs | Unresolved allowlist names (`ref`, `watchEffect`, …) resolve without `import`; local lookalikes still quiet |
| 2026-07-26 | `--reactivity-tui` | Interactive busiest-first browser for module facts; TTY-only; empty modules hidden until toggled |
| 2026-07-26 | TUI UX | Tab-focus panel scroll; humanized edge labels (`v-if → x`); `g` graph groups inbound reads by binding; `?` explains `@offset` |
| 2026-07-26 | TUI mouse | Click modules/panel to focus/select; help dismiss on click; wheel unchanged |
| 2026-07-26 | Digest structured spans | `--print-reactivity` JSON adds `binding_details` / `edge_details` / `scope_details` / `template_details` with byte spans + shared humanize labels |
| 2026-07-26 | Thin VS Code host | `editors/vscode` consumes CLI JSON only (TreeView / decorations / hover); not the #12 LSP surface |
| 2026-07-26 | Editor UTF-8→UTF-16 | VS Code decorations must convert byte spans; raw `positionAt(byteOffset)` shifts highlights after multi-byte prefixes |
| 2026-07-26 | Binding inspect | TUI `b`/Enter/right-click selects a binding: inbound readers + outbound deps; Esc/x clears. VS Code context menus mirror this |
| 2026-07-26 | Graph v7 `property` | Dependency edges carry member path (`props.count`); digest `to_path` + humanize; TUI/VS Code pick `props.*` with inbound filter |
| 2026-07-26 | Component nav (structural) | JSON `component_nav` + TUI `c` + VS Code tree from `ComponentUsage`/`AutoComponent` only — **not** prop dataflow |
| 2026-07-27 | Prop dataflow channel | `ReactiveDependencyKind::Prop` via `join_prop_flows` after component edges; structural nav unchanged |
