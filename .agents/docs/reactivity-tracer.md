# Reactivity tracer

`vue_vet_reactivity` is the Vue Vet-owned **static reactivity tracing library**.
Lint rules are the first consumer, not the capability ceiling. Crate-facing docs
live in [`crates/vue_vet_reactivity/README.md`](../../crates/vue_vet_reactivity/README.md);
this record holds product stance and completeness judgment.

Related: [architecture](./architecture.md), [gotchas](./gotchas.md),
[literature matrix](./research/reactivity-tracer-literature.md),
[science memo](./research/reactivity-tracer-science.md) (post-v27 ranking),
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
  input, and requires an interactive TTY. Inspecting a computed/effect binding
  shows the same “would Vue re-run?” `scope_details.summary` as `--explain-scope`.
  `editors/vscode` is a thin CLI consumer (not LSP).
- **Explain Scope** (`--explain-scope <query>`) is the multi-consumer “would Vue
  re-run this?” surface: pure `vue_vet_reactivity::explain_tracking_scope` over
  `TrackingScopeFact`, session orchestration, text/JSON reporters, and MCP
  `vue_vet_explain_scope` (same JSON as CLI `--format json`). Finding
  `--explain` / MCP `vue_vet_explain` attaches optional `tracking` when the
  diagnostic span sits inside a scope. Query: binding, `module:binding`,
  `@offset` (span start, else tightest covering — same as finding `--explain`),
  `callee@offset` (exact start). Contract types: `ScopeExplain` /
  `ScopeExplainDep` / `ScopeTrackReason` in `vue_vet_core` (additive on
  `FindingExplain`). `--print-reactivity` `scope_details[].summary` is the same
  one-line verdict. VS Code **Explain Scope** shells CLI `--explain-scope @offset`.
  `vue-vet --lsp` hover uses `file:@offset` and the same markdown.
  `--reactivity-tui` inspect of a scope-owning binding shows the same summary.

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

Contract version: **`REACTIVITY_GRAPH_VERSION = 34`**.

v34 records **writes inside sync Array/String HOF callbacks and
`toValue(() => …)` getters**, dual-path with those nested reads.
`then` / `nextTick` / `setTimeout`, wrong-index first-arg functions, and
identifier `list.map(fn)` stay quiet. Prior:
v33 records **`bag.field.value` writes** on a known composable instance
(same fact as a destructured `field.value` write). Replacing the ref
(`bag.field = …`), computed keys, unknown bags, and non-ref-like fields
stay quiet. Prior:
v32 treats a same-file **render identifier** as the Render body:
`render: renderFn` and `setup() { return renderFn }` collect the same
reads as the inline function. Imports, methods, and async/generator stay
quiet. Prior:
v31 peels **parens / TypeScript wrappers on watch sources** so
`watch((count))` / `watch(count as T)` / `watch((() => count.value))`
agree with the unwrapped form. Nested arrays still do not treat inner
arrows as source getters. Prior:
v30 classifies **pause/enable/resetTracking inside followed helpers**
(and after those calls return). `load()` that pauses then reads is
OutsideTracking, dual-path with inline `pauseTracking(); x.value`. Vue's
`shouldTrack` is process-global, so a helper that ends paused stays paused
for later sibling reads in the caller. Do not compare helper spans against
caller events by file order. Await-in-helper stays quiet (async helpers
are unfollowed). Prior:
v29 records **compound assignment and update writes** (`+=`, `++`, and
the other arithmetic/bitwise compounds). Logical `&&=` / `||=` / `??=`
stay quiet (they may not write). `assignment_only` includes `++` / `--`.
Prior:
v28 attaches **caller control-flow guards** to followed helper reads:
`computed(() => cond ? load() : 0)` classifies `load`'s reads as Conditional
(dual-path with `cond ? x.value : 0`). Both-arm `load()` stays Unconditional.
Guards inside the helper (early-exit / inner ternary) also classify. Prior:
v27 identifier getters; v26 helper-follow writes / `assignment_only`;
v25 helper-follow `uncertain_accesses`; v24 named API bag ambient-on-call;
v23 hard-read helper follow; v22…v7.

v27 treats a same-file local function **reference** as the tracking body:
`computed(load)`, `watchEffect(load)`, `watch(load)`, and
`computed({ get: load })` collect the same reads / uncertain / writes /
`assignment_only` as `computed(() => load())`. Imports, methods, and
async/generator stay quiet. Unused parameters on the function are allowed
(Vue invokes the getter with no args) — that is not the helper-follow
`load(1)` quiet path.

v24 models **named API bag ambient-on-call methods** via plugin-supplied
`NamedApiBag` rows (not hardcoded in the engine). Default catalog from
**`vue_vet_plugins`**: vue-i18n `t`/`d`/`n`/`rt`/`te` inject ambient field
reads (`locale` / `fallbackLocale` / `messages`) per `wrapWithDeps`; Nuxt
data destructure seeds. Product boundary auto-loads plugins.

### Tracer plugins (`vue_vet_plugins`)

| Concern | Location |
| --- | --- |
| Engine types / empty default catalog | `vue_vet_reactivity` (`NamedApiBag`, `TracerPlugin`, `TraceConfig`) |
| Ecosystem hardcode (Nuxt, vue-i18n) | **published** `vue_vet_plugins` |
| Auto-load | `vue_vet_oxc`, `vue_vet_project` (`ensure_default_plugins`), `vue_vet_session` |
| crates.io order | `core` → `reactivity` → `plugins` |

See [vue_vet_plugins README](../../crates/vue_vet_plugins/README.md) and
[architecture](./architecture.md) (`Reactivity tracer plugins`).

| Axis | Status | Covered (in-scope) | Remaining |
| --- | --- | --- | --- |
| A1 Bindings | complete | Vue primitives, aliases, `#imports`, bare Nuxt/auto-import allowlist, `defineModel`, **Vue Macros `defineModels` destructure → ModelRef locals**, `defineProps` (whole object **and Vue 3.5+ object-destructure locals → Reactive**), `withDefaults(defineProps())` same, `storeToRefs`, `useRoute`/`useRouter`, `unref`/`toValue`, module seeds, **factory call returns** (`Factory(Ref|Reactive)` from body / `.d.ts`), **`.d.ts` object-bag returns** (`{ field: Ref }` / same-file interface·type alias → destructure seeds), **typed `Ref`/`ComputedRef` parameters & declarators** (scope classification; nested locals span-resolved), **`useI18n` ambient + synthetic composer when only translators destructured** | whole-object `const models = defineModels()` without destructure stays quiet; pre-3.5 props destructure still flagged by `no-nonreactive-props-destructure` |
| A2 Scopes | complete | effects, computed getter/`{ get, set }`, **identifier getters** (`computed(load)` / `watchEffect(load)` / `watch(load)` / `{ get: load }` / **`render: renderFn`** / **`setup() { return renderFn }`**), watch sources + callback outside, effectScope `.run` + provenance, dispose, **Render** (options `render` / `setup`→render / functional export / same-file `defineComponent` factory+alias+one-hop forwarder); **bounded same-file zero-arg helper follow** into scope reads, **`uncertain_accesses`**, **writes**, and **`assignment_only`** | cross-file / async / args / method callees stay quiet |
| A3 Reads | complete | `.value` / members / bag.field / sync Array·String·`Array.from`·`JSON.parse` HOF / watch ref `.value` / `unref`·`toValue` / bare `watch(reactive)` deep root `*` / **peeled watch sources** (`watch((ref))` / `watch(ref as T)`) / **reads inside followed local helpers** / **uncertain accesses inside followed local helpers** / **writes / assignment-only inside followed local helpers** / **`+=` / `++` writes** / **`bag.field.value` instance writes** / **HOF / `toValue` getter writes** / **`useI18n` translator ambient deps** | — |
| A4 Conditions | complete | if / early-exit / ternary / short-circuit / switch roles; **all-path same `(binding, property)` on both ternary/if-else arms → no BranchTest** (under-approx hygiene: do not invent Conditional); **followed helper reads inherit caller guards** (`cond ? load() : 0`); pure checks in `trace/branch_hygiene.rs` | further control-flow depth is out of charter |
| A5 Boundaries | complete | after-await; pause/enable/resetTracking windows; **pause inside followed helpers + leak past the call**; nested `then`/`nextTick` outside; watch callback outside | — |
| A6 Modules | complete | composable bags + Factory + ValueBag + ComponentFactory + ExternalImport + `#nuxt-imports` seeds; **policy algebra** (below); **`return local = call()` → ForwardReturn**; bare auto-import callee resolve; pending empty-path composable fields | whole-object `v-bind` quiet; `#imports` virtual without body quiet |
| A7 Contract | complete | **v34** HOF / `toValue` getter writes; v33 composable-instance writes; v32 render identifier getters; v31 watch-source peel; v30 pause-in-helper; v29 compound/update writes; v28 caller guards on followed reads; v27 identifier getters; v26 helper-follow writes / `assignment_only`; v25 helper-follow `uncertain_accesses`; v24 useI18n translator ambient; v23 local zero-arg helper follow; v22…v7 as before; deterministic sort | — |
| Evidence | complete | Runtime oracle (≥99% recall on committed cases); deep-watch `*`; exhaustive local reads; key SFC E2E | — (prop flow is static unit/project; not an `onTrack` pair) |

### ExportState policy algebra (A6 linking)

Cross-module seeds cross only **finished** export states. Phase-one builds
per-module `locals: name → ExportState`; link-time fixed point refines
forwards and publishes seedable states.

| State | Seedable? | Meaning (under-approx) |
| --- | --- | --- |
| `Known(k)` | yes | Value is already a reactive binding of kind `k` |
| `Factory(k)` | yes | Call returns scalar reactive of kind `k` |
| `Composable(shape)` | yes | Call returns object bag (fields / open spread / pending) |
| `ValueBag` / `ValueFactory` | yes | Nested method bag |
| `ComponentFactory` | yes | Setup-forward `defineComponent` wrapper |
| `ForwardReturn(name)` | no (provisional) | Body/`typeof`/`return local=call()` → resolve `name` then re-enter |
| `ValueFactoryCall` / `GenericMethodInstantiate` | no until refined | Call markers |
| `DeclaredPlainObjectFactory` / `BodyUnwrappedState` | no alone | Provisional halves for Reactive factory merge |
| `Ambiguous` | no | Conflicting evidence |

**Local merge** (same name, multiple declare/defs — e.g. ambient overloads):

1. Existing `Factory` + new `Composable` → keep **Factory** (scalar default overload).
2. Existing `Composable` + new `Factory` → take **Factory**.
3. Existing `Known` + new Factory/Composable → keep **Known** (graph-seeded wins).
4. Otherwise last write wins.

**Declaration / implementation merge** (`.d.ts` + companion body, per name):

1. `DeclaredPlainObjectFactory` ↔ `BodyUnwrappedState` → `Factory(Reactive)`.
2. Provisional declaration + seedable impl → take impl.
3. Declaration `ForwardReturn` + impl Factory/Composable/ValueFactory/ComponentFactory
   → take impl (`Known` / `ValueBag` stay quiet here).
4. Orphan provisional half alone is retained; unrelated pairs leave declaration unchanged.

**Name resolve** for `ForwardReturn` / bag method forwards (depth-capped):

1. Working locals (recurse through nested `ForwardReturn`).
2. ES import → link `(module, source)` → resolved export of `imported`.
3. Bare auto-import → link `(module, "#nuxt-imports:{name}")` → export `name`.

**Ternary value exports** (`const x = cond ? arm1 : arm2`): only when **both**
arms are ref-like call results → `Known(k)` (mixed plain arms quiet).
Ref-like kinds live on `ReactiveBindingKind::is_ref_like` (core); same kind keeps
it, distinct ref-like kinds merge to `Ref` (shared `.value` tracking).

**Seed materialize** only acts on seedable export states; provisional / non-seedable
variants stay quiet (`!is_seedable`) — never invent consumer bindings.

**Pending bag fields**: `const { a } = useX(); return { b: a }` records pending
`(export_key=b, root=useX, path=[], field=a)`; empty `path` means resolve
`Composable` field on `root` (member paths still ValueBag walk).

**Publish barrier** (seed map only accepts seedable states):

1. Non-seedable → drop (never invent consumer seeds).
2. First publish of a name → insert.
3. Same-class bag refinement (`ValueFactory`/`ValueBag`/`Composable`) → replace.
4. Conflicting seedable classes → sticky `Ambiguous`.
5. Already `Ambiguous` → unchanged.

Axes A0–A7 can be **complete** while this algebra still gains **contract
refinements** — refinements bump `REACTIVITY_GRAPH_VERSION` / project
`CONVENTIONS_VERSION`, not a new axis.

Executable merge/seedable/name-resolve/pending/publish/refine checks live in
`crates/vue_vet_reactivity/src/trace/summary/export_lattice.rs` (pure, no AST).
`link.rs` is the impure adapter (facts, links, fixed-point queue).

### In-scope complete checklists

| Axis | Checklist (all required for `complete`) |
| --- | --- |
| A1 | ✅ Allowlist primitives + macros + pinia/router + auto-import + module seeds + factory call returns; local lookalikes quiet; unit/oracle cover |
| A2 | ✅ effect / computed / watch / effectScope.run(+provenance) / dispose / Render scopes; no invented effectScope; **same-file identifier getters** (`computed(load)` and watch/effect equivalents); **same-file zero-arg helper follow (depth≤2) for hard reads, uncertain, writes, and assignment_only** |
| A3 | ✅ Member/HOF/unref·toValue reads; watch ref `.value`; **deep root `*` for bare `watch(reactive)`** (not per-key invention); helper-body ambient reads, uncertain, **and writes** (including `+=` / `++`, `bag.field.value`, and sync HOF / `toValue` getter writes); **useI18n `t`/`d`/`n`/`rt`/`te` ambient** |
| A4 | ✅ Guard roles + all-path same-identity branch reads (`branch_hygiene`); **followed helpers inherit caller guards**; no further CF depth for recall |
| A5 | ✅ After-await classification; pause/enable/resetTracking windows; **pause inside followed helpers**; nested callback outside-tracking; watch callback outside |
| A6 | ✅ Composable/instance/dual-script/provide-inject; Factory/Composable/ValueBag/ComponentFactory; policy algebra (above); bare `#nuxt-imports` seeds + ForwardReturn resolve; external summaries; static `:prop` edges |
| A7 | ✅ Versioned graph (**v34**); deterministic sort; `property`/`to_path`; **`{module}:{name}@{offset}` `to_id`** |
| Evidence | ✅ `just oracle` ≥99% recall on committed cases; exhaustive local reads; key SFC E2E |

### In-scope remaining (this epic)

None for axis completeness.

**2026-08-10 milestone (contract hygiene closed):** pure ExportState lattice
(`export_lattice`, #161–#167), core `ReactiveBindingKind` ref-like contract
(#168), pure A4 `branch_hygiene` (#169), and multi-consumer
`uncertain_accesses` on digests (#170). Oracle green.

**2026-08-10 evidence refinement (v23):** Elk `StatusReactedBy` —
`computed(() => load())` with reads only in same-file `load` — proved A2/A3
missed ambient callee tracking. Bounded same-file zero-arg helper follow
(depth 2, no async/generator/args/import/method).

**2026-08-10 evidence refinement (v24):** Elk `PublishWidget` —
`const { t } = useI18n(); computed(() => t(…))` is **not** a hard TP. vue-i18n
`wrapWithDeps` tracks locale/messages. Modeled as table-driven `NamedApiBag`
ambient-on-call methods (not case-by-case `has_translator` flags): contract row
for useI18n; seed registers method handles; call injects precomputed ambient
reads.

**2026-08-21 contract refinement (v25):** v23 helper follow applied only to
hard reads. `computed(() => isCoarse.value)` recorded `uncertain_accesses`
while `function load() { return isCoarse.value }; computed(() => load())`
left both reads and uncertain empty — absence rules then fired a
**confident** no-dependency. Dual-path: `collect_uncertain_scope_accesses`
now shares `local_zero_arg_callees_in_scope` (depth≤2, skip async /
generator / args / `then()`-only).

**2026-08-21 contract refinement (v26):** the same dual-path hole existed
for **writes** and **`assignment_only`**. Inline
`computed(() => { b.value = a.value; return a.value })` fired
`no-side-effects-in-computed`, while
`function load() { b.value = a.value; return a.value }; computed(() => load())`
had empty writes. `watchEffect(() => { assign() })` stayed
`assignment_only == false` so `prefer-computed` / PreferWatchSingle were
silent. Reads, uncertain, and writes share `follow_local_callees` (depth≤2,
skip async / generator / args; reads mark `then()`-only outside, the others
skip it). `is_assignment_only_followed` walks statements with the same
`local_function_id` + async skip. CSS `<style>` `v-bind(ident)` joins as
`TemplateExpressionFact { surface: "style" }` so unused-computed does not
FP; style-only ident edits refresh those expressions without adding style
to `SfcBlockRevisions`.

**2026-08-25 contract refinement (v27):** helper follow covered
`computed(() => load())` but `callback_parts` only matched inline
arrow/function/`{ get }`. Vue accepts a function reference as the getter
(`computed(load)`, `watchEffect(load)`, `watch(load)`, `{ get: load }`) and
tracks that body. Those forms created **no scope**. `local_getter_parts`
resolves a peeled identifier through `local_function_id` (skip
async/generator/import/method). Parameters do not disqualify — Vue calls
the getter with no args. Oracle: `computed-fn-ref`, `watch-source-fn-ref`.

**2026-08-26 contract refinement (v28):** followed helper reads ignored
caller control flow. `path_guards` walked ancestors of the read, so
`computed(() => cond ? load() : 0)` invented Unconditional. Each follow hop
now records call sites; classify uses owning-function guards plus call-site
proxies so `branch_hygiene` can see both-arm `load()`. Dual-path with
`cond ? x.value : 0`. Oracle: `computed-helper-ternary`.

**2026-08-26 contract refinement (v29):** `+=` / `++` (and other
arithmetic/bitwise compounds) were reads-only. `writes.rs` required
`operator.is_assign()` (`=`). That left `assignment_only` true with empty
`writes`, so `no-side-effects-in-computed` / `prefer-computed` /
self-trigger stayed quiet. Logical `&&=` / `||=` / `??=` stay quiet.

**2026-08-26 contract refinement (v30):** pause inside a followed helper
was ignored (`scope_owns_pause_call` stopped at the first nested
`Function`; IR was the caller `scope_id`). File-linear comparison would
also mis-order a helper declared above the effect. Classify now uses
per-function pause IR + caller hops, and projects a helper's last
pause/resume onto the call end (Vue `shouldTrack` leaks). Oracle:
`pause-tracking-helper`. Nested `pause; pause; enable` is still last-event,
not a stack/counter.

**2026-08-26 contract refinement (v31):** watch-source collection matched
Identifier / member / array / inline getter on the raw argument.
`local_getter_parts` and uncertain watch sources already peeled, so
`watch((count))` / `watch(count as T)` invented an empty source (and a
confident `no-empty-watch-sources` finding). Peel before classifying.
Nested `watch([[() => x.value]])` still does not treat the inner arrow as
a getter. Oracle: `watch-source-parens`.

**2026-08-26 contract refinement (v32):** `function_like_body` accepted only
inline arrows / function expressions. `render: renderFn` and
`setup() { return renderFn }` created no Render scope while
`computed(load)` already did. Resolve through `local_getter_parts`
(same-file, skip async/generator). Graph-vs-graph is the gate — the
oracle harness has no component `onTrack` for render.

**2026-08-26 contract refinement (v33):** writes matched `reactive_bindings`
only. Reads already resolved `bag.field.value` via `composable_instances`.
`computed(() => { bag.field.value = 1 })` therefore missed the write
`no-side-effects-in-computed` needs. Record the same fact as a destructured
`field.value` write. `bag.field = …`, computed keys, unknown bags, and
non-ref-like fields stay quiet.

**2026-08-26 contract refinement (v34):** writes dropped every nested
function. Reads already stayed inside sync Array/String HOF callbacks and
`toValue(() => …)` getters. `list.value.map(() => { t.value = 1 })` inside
computed therefore missed the write. Share HOF / `toValue` / deferred
classification via `sync_tracking_owns_node`. `then` / `nextTick` /
`setTimeout`, first-arg `Array.from(() => …)`, and identifier `list.map(fn)`
stay quiet (same as reads). Graph-vs-graph is the gate — `onTrack` does
not see writes.

**Do not** auto-continue pure extracts, Elk/corpus KPI chasing, or a11y as
tracer A0–A7. Next tracer work needs **evidence** first:

1. **Contract refinement** — invent Conditional / blocked seed / dual-path
   inconsistency → fix + unit/oracle + **`REACTIVITY_GRAPH_VERSION` bump** +
   PCR lattice update. Ranked candidates after v34 (measurement / locality /
   consumer polish) live in
   [science memo](./research/reactivity-tracer-science.md).
   Caller-guard-on-follow is dual-path hygiene, not "further A4 for recall."
2. **Consumer surface** — rules / explain / TUI / VS Code using facts already
   on the graph (optional product polish; not axis work).
3. **Otherwise stop** — A0–A7 complete is a valid resting point.

Product a11y / project-import polish is **not** an A0–A7 axis (catalog
`parity`); keep it off the tracer epic narrative.

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
| 2026-08-10 | Digest surfaces soft evidence | `--print-reactivity` `scope_details.uncertain_accesses` + text labels `maybe:a,b` for multi-consumer digests (same under-approx contract as absence rules) |
| 2026-08-10 | Contract-hygiene series closed | #161–#170: lattice pure + ref-like + branch_hygiene + digest soft evidence; **stop pure-extract loops** until evidence; consumers optional |
| 2026-07-29 | Absence-rule strategy | Prefer hard evidence (Factory, `const alias = ref`, nested `.value` roots, watch-source uncertain); absence pathologies share `(maybe)` when only soft evidence remains |
| 2026-07-29 | `.d.ts` object-bag returns (#118) | Declared `{ width: Ref }` / same-file `interface`·`type` return shapes → `ExportState::Composable`; fixes VueUse `useElementSize` destructure → `no-empty-watch-sources` FP |
| 2026-07-29 | Plain-object Reactive factory (#119) | Declared plain object (no Ref fields) + body `return <call>(...).value` (`#imports`/unresolved) or `return reactive(...)` → `Factory(Reactive)`; `.nuxt/imports.d.ts` bare calls → `#nuxt-imports:` ExternalReactivityRoot; companion `.js` merge only for provisional halves (+ size cap); fixes Nuxt `useColorMode` → `no-empty-watch-sources` FP |
| 2026-07-29 | Nuxt imports importer resolve | Bare `#nuxt-imports:` seeds resolve from the **declaring** dts (`imports.d.ts` vs `types/imports.d.ts`); prefer re-export map when both list the same name. Types-map overwrite + fixed importer was the real-app `colorMode` FP after #119 |
| 2026-07-31 | VueUse shared composable | `createSharedComposable` / `createGlobalState` from `@vueuse/core` forward the factory return bag (`Fn` → `Fn`) so destructured fields like `hasPermission` seed |
| 2026-08-10 | Nuxt data / route slice / i18n | `await useAsyncData` destructure Ref fields; `useRoute().params|query|meta` Reactive; `useI18n` locale/locales |
| 2026-08-10 | Bare auto-import Known | Nuxt/Vite map links for free idents (not only calls); `ExportState::Known` seeds without import span; unresolved refs match seed by name |
| 2026-08-10 | Return local = call() forward | `const x = useY(); return x` → `ForwardReturn(useY)`; resolve via ES import **and** `#nuxt-imports:{name}` |
| 2026-08-10 | External bare `export *` | External summary follows bare re-export sources (`export * from 'pkg'`), not only `typeof` forwards |
| 2026-08-10 | Ternary ref-like init | Both arms ref-like calls → seed / export `Known`; mixed plain arm quiet |
| 2026-08-10 | Overload Factory≻Composable | Ambient scalar + controls-bag overloads keep `Factory` for default call form |
| 2026-08-10 | Pending empty-path field | `const { a } = useX(); return { b: a }` → link-time Composable field on `useX` |
| 2026-08-10 | All-paths branch reads | Same `(binding, property)` on both ternary/if-else arms → drop BranchTest |
| 2026-08-10 | Export lattice + versions | Lattice written as A6 contract; graph **v22** / conventions **v14** |
| 2026-08-10 | Same-file zero-arg helper follow | `collect_scope_reads` follows bare `f()` to local `function`/`const f = () =>` (depth≤2, skip async/generator); graph **v23**; Elk StatusReactedBy-class FP |
| 2026-08-10 | Named API bag ambient-on-call | Engine consumes plugin-supplied `NamedApiBag` rows (ambient-on-call methods); graph **v24**; Elk PublishWidget without-dep FP |
| 2026-08-21 | Helper-follow uncertain | `uncertain_accesses` follows the same zero-arg helpers as hard reads; `then()`-only stays quiet; graph **v25**; dual-path with inline `(maybe)` |
| 2026-08-21 | Helper-follow writes | `writes` + `assignment_only` follow the same zero-arg helpers; `then()`-only stays quiet; graph **v26**; dual-path with inlined assignment / `prefer-computed` |
| 2026-08-25 | Identifier getters | `computed(load)` / `watchEffect(load)` / `watch(load)` / `{ get: load }` use the local function as the tracking body; import/method/async quiet; graph **v27**; dual-path with `computed(() => load())` |
| 2026-08-25 | Science memo after v27 | Ranked dual-path / measurement / locality / consumer work. Literature §K refreshed (July column was stale). `ExportState` prose is policy algebra, not a math lattice. No graph bump (docs). |
| 2026-08-26 | Caller guards on follow | Followed helper reads inherit caller ternary/if/early-exit/short-circuit; both-arm helper calls stay Unconditional; graph **v28**; dual-path with inline `cond ? x.value : 0` |
| 2026-08-26 | Compound / update writes | `+=` / `++` record write facts like `=`; logical `&&=` quiet; `assignment_only` includes updates; graph **v29** |
| 2026-08-26 | Pause in followed helper | Pause/enable/reset inside `load()` + leak past the call; per-function IR (no file-offset mix); graph **v30**; oracle `pause-tracking-helper` |
| 2026-08-26 | Watch-source peel | `watch((count))` / `watch(count as T)` / parenthesized getters agree with the bare form; nested arrays stay identifier-only; graph **v31**; oracle `watch-source-parens` |
| 2026-08-26 | Render identifier getters | `render: renderFn` / `setup() { return renderFn }` use the local function as the Render body; import/method/async quiet; graph **v32**; dual-path with inline `render() { … }` |
| 2026-08-26 | Composable-instance writes | `bag.field.value = …` records the same write as destructured `field.value`; replace / computed key / unknown bag quiet; graph **v33** |
| 2026-08-26 | HOF / toValue getter writes | sync Array/String/`toValue` callbacks record writes like inlined assignments; `then` / first-arg / `map(fn)` quiet; graph **v34** |
| 2026-08-26 | Helper-call oracle + consumer polish | `computed(() => load())` oracle next to `computed-fn-ref`; module-qualified `--explain-scope` skips other graphs; MCP `vue_vet_scan` ships CLI reactivity totals; ExportState prose is policy algebra. No graph bump. |
| 2026-08-21 | Helper-follow walk unify | Reads / uncertain / writes share `follow_local_callees`; drop unused `local_function_id` name arg. No graph version bump (same facts). |
| 2026-08-21 | CSS `v-bind` join | `<style>` `v-bind(ident)` / quoted ident → `TemplateExpressionFact.surface = "style"`; style-only ident edits refresh without adding style to revisions |
| 2026-08-10 | Tracer plugins crate | Ecosystem hardcode (Nuxt data bags, vue-i18n `useI18n`) lives in published `vue_vet_plugins`; engine has no Nuxt/i18n names; Oxc/project/session **auto-load** defaults; crates.io order core→reactivity→plugins; docs: crate README + install library table |
| 2026-08-10 | `defineProps` destructure | Object-pattern + rest locals seed `Reactive` (Vue 3.5); `withDefaults` same |
| 2026-08-10 | Vue Macros `defineModels` | Setup-only; object-destructure locals seed `ModelRef` |
| 2026-08-20 | MCP explain-scope consumer | `vue_vet_explain_scope` returns the same `ScopeExplain` JSON as CLI `--explain-scope`; finding `vue_vet_explain` already nests `tracking` |
| 2026-08-20 | Explain-scope `@offset` covering | Dual-path: finding `--explain` used `scope_covering_span`; `@offset` was start-only. Bare / `module:@offset` now start-exact then tightest covering (length 1). `callee@offset` stays start-exact. Digest `scope_details.summary` + VS Code command. No graph version bump (query + digest field, not graph facts). |
| 2026-08-20 | LSP explain-scope hover | `vue-vet --lsp` hover maps UTF-16 caret → byte offset → `file:@offset` `ScopeExplain` markdown. Session `explain_scope` reuses the committed full snapshot after DiagnosticsOnly publish. No graph version bump. |
| 2026-08-20 | TUI inspect ScopeExplain | `--reactivity-tui` inspect keeps `scope_details` (was label-only) and shows the same `summary` as `--explain-scope` for the owning binding |
| 2026-07-31 | `inject(key) as Ctx` bag | Peel `TSAsExpression` to find the declarator; seed asserted Ref-field interface when provide offer is unknown; `return ctx` after assertion exports the bag (map-context helpers) |
| 2026-07-31 | Generic context factory | `return value as T` (enclosing type param) → `MethodGeneric`; typed call destructure `const { useInject: useX } = factory<Ctx>(…)` → link-time `Composable` from the matching type argument (no name allowlist) |
| 2026-07-31 | `expr as Ref` declarator | `const modelValue = useVModel(…) as Ref<T>` seeds a Ref binding from the outermost assertion (same under-approx as `: Ref` annotations) |
| 2026-07-31 | Typed function-callback formals | Callee param typed as `(state: ComputedRef\|Ref\|…) => …` publishes `TypedCallbackParamSlots`; call-site arrow/function args seed those formals (cross-module + barrels); no callee-name allowlist |
| 2026-07-31 | `RemovableRef` + `typeof` re-export | VueUse `RemovableRef` → Factory(Ref); `export const useX: typeof useY` → `ForwardReturn`; external follow loads bare `typeof` target packages (budgeted) |
| 2026-07-31 | Optional `{ value?: T }` duck | Type literal whose only member is optional `value?` → Ref (param/return); required `{ value: T }` stays quiet |
| 2026-07-31 | Sync HOF plain `.value` | Callback params of sync Array/String HOF methods skip `uncertain_accesses` for `.value` (select-option fields, not Ref unwrap); untyped composable formals still maybe |
| 2026-07-31 | Imported value-factory call | `const api = createApi()` with imported `createApi` → `ValueFactoryCall` → link-time `ValueBag` re-snapshot; nested hooks + `api.ns.useX()` destructure seeds; wrapper `return { isLoading }` via `PendingValueBagField` |
| 2026-07-30 | Vite `auto-imports.d.ts` | Load root / `src/auto-imports.d.ts` after Nuxt maps; parse `typeof import('…')['name']`; single-file scans walk up to nearest `package.json` so nested IDE paths still load the map; composable `return { …, ...reactiveBag }` open-spread seeds unknown destructure keys as Ref — fixes unplugin-auto-import / vue-query `isLoading` FPs |
| 2026-07-30 | props + shape forwarding | Seed Vue `defineComponent` / `setup(props)` bags; setup-forward wrappers → `ExportState::ComponentFactory` (cross-module + size-capped package `exports.import` body); opaque helpers quiet; mapped/`toRefs`/return-call composable shapes + nested value-bag member calls (no query name allowlists); barrel named imports mark component-name targets used |
| 2026-07-30 | options-object callback bags | Export summary carries `(argIndex → prop → Ref bag)` from options params (`setup?: (ctx) =>`); call-site `setup({ values })` seeds ObjectPattern fields; interface `extends` merges with visited+depth guards (avoid project-wide stack overflow) |
| 2026-07-30 | Return reactive spreads | `return { …, ...bag }` with same-function `bag.field.value` evidence → `ComposableShape.open_reactive_spread`; unknown destructure keys seed as `Ref` (vue-query `isLoading` via `...queryResult`) |
| 2026-07-29 | Analysis enrichment passes | Nuxt bare seeds + provisional Factory companion merge extracted as compile-time IR passes (`vue_vet_project::passes`); diagnostic `Rule` stays separate; no user/AST plugin host |
| 2026-07-29 | Enrichment Pass::run | Dropped empty metadata trait; `ENRICHMENT_STEPS` checklist + inherent `Pass::run`; `ExternalSummaryLoadPass` owns load; SummaryMerge at per-module completion |
| 2026-07-29 | Project crate pipeline | Split `vue_vet_project` monolith into `model`/`context`/`structural`/`passes`/`pipeline`/`layers`/`rules`/`state`; thin `lib` façade |
| 2026-07-29 | Companion `.js` over-merge | `needs_implementation_merge` must not treat “no finished seeds” as incomplete — that parsed `typescript.js` (~9 MB) for `import … from 'typescript'` and stalled `pixi-heatmap/docs` (~20 s) |
