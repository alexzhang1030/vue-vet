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
- **Rules that need this graph are the product differentiator** (catalog tier
  `tracer`). Template Essential / a11y / after-await registrars are `parity`
  completeness — valuable, but not what only Vue Vet can do. See
  [`docs/rules/README.md`](../../docs/rules/README.md).
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

Contract version: **`REACTIVITY_GRAPH_VERSION = 9`** (v8 + **`TrackingScopeKind::Render`**
for recognized component render bodies; v8 module-qualified `to_id`
`{module}:{name}@{offset}`; v7 `property` / `to_path` retained; bare `to` kept
for rule matching).

| Axis | Status | Covered (in-scope) | Remaining |
| --- | --- | --- | --- |
| A1 Bindings | complete | Vue primitives, aliases, `#imports`, bare Nuxt/auto-import allowlist, `defineModel`, `defineProps`, `withDefaults(defineProps())`, `storeToRefs`, `useRoute`/`useRouter`, `unref`/`toValue`, module seeds, **factory call returns** (`Factory(Ref|Reactive)` from body / `.d.ts`), **`.d.ts` object-bag returns** (`{ field: Ref }` / same-file interface·type alias → destructure seeds), **typed `Ref`/`ComputedRef` parameters & declarators** (scope classification; nested locals span-resolved) | — |
| A2 Scopes | complete | effects, computed getter/`{ get, set }`, watch sources + callback outside, effectScope `.run` + provenance, dispose, **Render** (options `render` / `setup`→render / functional export / same-file `defineComponent` factory+alias+one-hop forwarder) | cross-file opaque factories stay quiet unless options structure is local |
| A3 Reads | complete | `.value` / members / bag.field / sync Array·String·`Array.from`·`JSON.parse` HOF / watch ref `.value` / `unref`·`toValue` / bare `watch(reactive)` deep root `*` | — |
| A4 Conditions | complete | if / early-exit / ternary / short-circuit / switch roles (fact metadata; diagnostics are scope-aware Conditional rules, not per-role ids — #136) | — (no further depth) |
| A5 Boundaries | complete | after-await; pause/enable/resetTracking windows; nested `then`/`nextTick` outside; watch callback outside | — |
| A6 Modules | complete | composable object bags + **scalar `Factory` returns** + **declared object-bag return types** + **plain-object + unwrapped-call → `Factory(Reactive)`**; instance bags; dual script; provide/inject unique-key; static `:prop` / `v-model` / `ident` / `ident.value` / static member + optional chains → child `props` Prop edges; **on-demand ExternalImport summaries** (`.d.ts` + companion `.js` for provisional halves, size-capped; re-export follow; not lint targets); **bare `.nuxt/imports.d.ts` / Vite `auto-imports.d.ts` → `#nuxt-imports:` seeds** | whole-object `v-bind` stays quiet; `#imports` virtuals stay quiet without a concrete file body |
| A7 Contract | complete | v9 Render scopes; v8 module-qualified `to_id`; v7 `property` / `to_path`; deterministic sort | — |
| Evidence | complete | Runtime oracle (≥99% recall on committed cases); deep-watch `*`; exhaustive local reads; key SFC E2E | — (prop flow is static unit/project; not an `onTrack` pair) |

### In-scope complete checklists

| Axis | Checklist (all required for `complete`) |
| --- | --- |
| A1 | ✅ Allowlist primitives + macros + pinia/router + auto-import + module seeds + factory call returns; local lookalikes quiet; unit/oracle cover |
| A2 | ✅ effect / computed / watch / effectScope.run(+provenance) / dispose / Render scopes; no invented effectScope |
| A3 | ✅ Member/HOF/unref·toValue reads; watch ref `.value`; **deep root `*` for bare `watch(reactive)`** (not per-key invention) |
| A4 | ✅ Existing guard roles; no further control-flow deepening |
| A5 | ✅ After-await classification; pause/enable/resetTracking windows; nested callback outside-tracking; watch callback outside |
| A6 | ✅ Composable/instance/dual-script/provide-inject; **Factory scalar + Reactive**; **`.d.ts` / annotated object-bag returns**; **mapped Ref types → open spread**; **`return toRefs` / `return call()` shape forward**; **nested ValueBag member calls**; **plain-object + `call().value` merge**; **external package summaries** (+ companion js); **bare Nuxt / Vite auto-imports.d.ts seeds**; **static `:prop` → child props bag edges** |
| A7 | ✅ Versioned graph (v9 Render); deterministic sort; `property`/`to_path`; **`{module}:{name}@{offset}` `to_id`** |
| Evidence | ✅ `just oracle` ≥99% recall on committed cases; exhaustive local reads; key SFC E2E |

### In-scope remaining (this epic)

None — deep-watch `*`, v8 `to_id`, and static prop flow shipped. Further breadth is out of scope below.

### Out of scope / A0 stop (never blocks complete)

| Gap | Why |
| --- | --- |
| Further A4 control-flow depth | Already deep; wrong axis for recall |
| Whole-program JS soundness / full alias analysis | Charter: under-approx Vue tracking only |
| App Tree provide/inject | Unique-key index is the in-scope model |
| Long-tail reactivity APIs with no analyzable return | Quiet failure; prefer Factory return-kind analysis (body / `.d.ts`) over name allowlists; expand allowlist only with oracle evidence when analysis cannot see a return |
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
| 2026-07-28 | Bounded module lifecycle | `max_workers` caps both phases; Oxc supplies prepared phase-one facts; only seeded consumers reparse |
| 2026-07-28 | Incremental partial module lifecycle | `ModuleTraceState` reuses unchanged seed plans/final graphs; per-module issues preserve healthy cross-module links |
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
| 2026-07-27 | Prop flow expression roots | `v-model` → `modelValue`; parent expr roots for bare / `.value` / single `ident.member` |
| 2026-07-27 | Prop flow multi-hop roots | Static `ident.a.b` chains join to root binding only; calls/optional/brackets stay quiet |
| 2026-07-27 | Prop flow optional chains | `ident?.a?.b` normalizes to the same root join; `?.()` / `?.[…]` stay quiet |
| 2026-07-27 | Template instance optional chains | `bag?.field` / `bag?.field?.value` join composable instance fields like dotted forms |
| 2026-07-27 | Oracle findIndex / reduceRight / reset | Evidence cases for already-supported sync HOFs + `resetTracking` window |
| 2026-07-27 | Oracle findLast / replaceAll / toSorted | Evidence for remaining allowlisted sync Array/String callback HOFs |
| 2026-07-29 | Factory return kinds (#115) | `ExportState::Factory`; body `return ref` + `.d.ts` `Ref`/`ComputedRef` return types; ExternalImport on-demand summaries (not lint targets); fixes VueUse `useMediaQuery` → `no-computed-without-dependency` FP |
| 2026-07-29 | Uncertain accesses `(maybe)` | Scope `uncertain_accesses` for unclassified `.value`/`unref`/`toValue`; `no-computed-without-dependency` labels `(maybe: name)` instead of silence or invented edges |
| 2026-07-29 | Absence-rule strategy | Prefer hard evidence (Factory, `const alias = ref`, nested `.value` roots, watch-source uncertain); absence pathologies share `(maybe)` when only soft evidence remains |
| 2026-07-29 | `.d.ts` object-bag returns (#118) | Declared `{ width: Ref }` / same-file `interface`·`type` return shapes → `ExportState::Composable`; fixes VueUse `useElementSize` destructure → `no-empty-watch-sources` FP |
| 2026-07-29 | Plain-object Reactive factory (#119) | Declared plain object (no Ref fields) + body `return <call>(...).value` (`#imports`/unresolved) or `return reactive(...)` → `Factory(Reactive)`; `.nuxt/imports.d.ts` bare calls → `#nuxt-imports:` ExternalReactivityRoot; companion `.js` merge only for provisional halves (+ size cap); fixes Nuxt `useColorMode` → `no-empty-watch-sources` FP |
| 2026-07-29 | Nuxt imports importer resolve | Bare `#nuxt-imports:` seeds resolve from the **declaring** dts (`imports.d.ts` vs `types/imports.d.ts`); prefer re-export map when both list the same name. Types-map overwrite + fixed importer was the real-app `colorMode` FP after #119 |
| 2026-07-30 | Vite `auto-imports.d.ts` | Load root / `src/auto-imports.d.ts` after Nuxt maps; parse `typeof import('…')['name']`; single-file scans walk up to nearest `package.json` so nested IDE paths still load the map; composable `return { …, ...reactiveBag }` open-spread seeds unknown destructure keys as Ref — fixes unplugin-auto-import / vue-query `isLoading` FPs |
| 2026-07-30 | props + shape forwarding | Seed `defineComponent` / `defineTypedComponent` / `setup(props)` bags; mapped/`toRefs`/return-call composable shapes + nested value-bag member calls (no query name allowlists); barrel named imports mark component-name targets used |
| 2026-07-30 | Return reactive spreads | `return { …, ...bag }` with same-function `bag.field.value` evidence → `ComposableShape.open_reactive_spread`; unknown destructure keys seed as `Ref` (vue-query `isLoading` via `...queryResult`) |
| 2026-07-29 | Analysis enrichment passes | Nuxt bare seeds + provisional Factory companion merge extracted as compile-time IR passes (`vue_vet_project::passes`); diagnostic `Rule` stays separate; no user/AST plugin host |
| 2026-07-29 | Enrichment Pass::run | Dropped empty metadata trait; `ENRICHMENT_STEPS` checklist + inherent `Pass::run`; `ExternalSummaryLoadPass` owns load; SummaryMerge at per-module completion |
| 2026-07-29 | Project crate pipeline | Split `vue_vet_project` monolith into `model`/`context`/`structural`/`passes`/`pipeline`/`layers`/`rules`/`state`; thin `lib` façade |
| 2026-07-29 | Companion `.js` over-merge | `needs_implementation_merge` must not treat “no finished seeds” as incomplete — that parsed `typescript.js` (~9 MB) for `import … from 'typescript'` and stalled `pixi-heatmap/docs` (~20 s) |
