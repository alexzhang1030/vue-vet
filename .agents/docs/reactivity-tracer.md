# Reactivity tracer

`vue_vet_reactivity` is the Vue Vet-owned **static reactivity tracing library**.
Lint rules are the first consumer, not the capability ceiling. Crate-facing docs
(install, quick start, module graph API, what the graph contains) live in
[`crates/vue_vet_reactivity/README.md`](../../crates/vue_vet_reactivity/README.md);
this record holds product stance, the completeness judgment, and the A6 policy
algebra. Traps are in [gotchas](./gotchas.md#reactivity-tracer).

Related: [architecture](./architecture.md),
[literature matrix](./research/reactivity-tracer-literature.md),
[science memo](./research/reactivity-tracer-science.md).

## Product stance

- Approximate Vue's **synchronous tracking semantics** with static facts. Do
  not execute components, effects, or Proxies for product analysis; runtime is
  an **oracle** for tests only (`just oracle*` recipes).
- Prefer **under-approximation + quiet failure** over inventing edges. Tracking
  scopes record `unknown_calls` / `follow_truncated` / `uncertain_accesses`;
  absence rules and Explain `analysis_complete` require those to be empty
  before claiming Vue will not re-run.
- Keep Vue Vet-owned serializable contracts independent of Oxc and Vize types.
  Contract identity is `REACTIVITY_GRAPH_VERSION` (`vue_vet_core`); cache
  identity is `RULESET_VERSION` (`crates/vue_vet_cache/src/lib.rs`). Contract
  refinements bump the graph version; new source-contract / lifetime facts
  compose onto the file-fact catalog without a graph bump.
- **Rules that need this graph are the product differentiator** (catalog tier
  `tracer`); template Essential / a11y are `parity` completeness. See
  [`docs/rules/README.md`](../../docs/rules/README.md).
- Multi-consumer surfaces: the CLI **Reactivity** digest after the score line
  (`--print-reactivity` structured `*_details`, `--reactivity-tui` busiest-first
  browser, JSON `reactivity` totals), **Explain Scope** (`--explain-scope
  <query>`: binding, `module:binding`, `@offset`, `callee@offset`; pure
  `explain_tracking_scope` over `TrackingScopeFact`, contract types
  `ScopeExplain` / `ScopeExplainDep` / `ScopeTrackReason` in `vue_vet_core`),
  LSP hover (`file:@offset`), MCP `vue_vet_explain_scope`, and the thin
  `editors/vscode` host. All return the same one-line verdict.

## What "complete" means

Completeness is coverage of **in-scope** Vue synchronous tracking semantics for
the product charter — not whole-program JavaScript soundness. Long-tail APIs
and general alias analysis stay **out of scope** and do not block complete
(bare `const alias = known` is `ReactiveBindingFact.alias_of`, not an alias IR).

| Axis | Question | Status | Covered (in-scope) | Remaining |
| --- | --- | --- | --- | --- |
| A0 Semantics | Which Vue tracking rules are in scope? | complete | Synchronous tracking; after-await / pause / deferred boundaries | whole-program JS is the A0 stop |
| A1 Bindings | Which locals are reactive, with what kind? | complete | Vue primitives, aliases, `#imports` / bare auto-import allowlist, `defineModel`, Vue Macros `defineModels` destructure → `ModelRef`, `defineProps` (object and 3.5 destructure → Reactive), `withDefaults`, `storeToRefs`, `useRoute` / `useRouter`, `unref` / `toValue`, module seeds, `Factory(Ref\|Reactive)` returns from body / `.d.ts`, `.d.ts` object-bag returns, typed `Ref` / `ComputedRef` parameters and declarators, `useI18n` ambient | whole-object `const models = defineModels()` quiet |
| A2 Scopes | Which regions perform tracking? | complete | effects, computed getter / `{ get, set }`, identifier getters (`computed(load)`, `render: renderFn`), watch sources (callback outside), `effectScope.run` with provenance, Render bodies; bounded same-file zero-arg helper follow (depth ≤ 2) for reads / uncertain / writes / `assignment_only` | cross-file / async / args / method callees are `unknown_calls` |
| A3 Reads | Which reactive reads occur, with what path? | complete | `.value`, members, `bag.field`, sync Array / String / `Array.from` / `JSON.parse` HOF callbacks, watch ref `.value`, bare `watch(reactive)` deep root `*`, peeled sources, compound / update writes, instance writes, HOF / `toValue` getter writes | — |
| A4 Conditions | Under which conditions is a read demandable? | complete | if / early-exit / ternary / short-circuit / switch guard roles; all-path same `(binding, property)` → no BranchTest (`trace/branch_hygiene.rs`); followed helper reads inherit caller guards | further control-flow depth is out of charter |
| A5 Boundaries | Where does sync tracking end? | complete | after-await; pause / enable / resetTracking windows (including inside followed helpers, leaking past the call); nested `then` / `nextTick` outside; watch callback outside | — |
| A6 Modules | How do composables and exports seed consumers? | complete | composable bags, Factory, ValueBag, ComponentFactory, ExternalImport, `#nuxt-imports` seeds; policy algebra below; `ForwardReturn`; provide/inject unique-key index; static `:prop` edges | whole-object `v-bind` quiet; `#imports` virtual without body quiet |
| A7 Contract | Is the graph versioned, deterministic, multi-consumer stable? | complete | versioned graph; deterministic sort; `property` / `to_path`; `{module}:{name}@{offset}` `to_id`; lost-notification `source_views` / `notification_bypasses` | — |
| Evidence | — | complete | `just oracle` ≥ 99 % recall on committed `onTrack` cases; exhaustive local fixture reads; key SFC E2E; graph-vs-graph gates where `onTrack` cannot see (writes, render) | prop flow is static unit / project evidence |

Every fact family must be **dual-path**: the inline form and the helper-backed
/ peeled / identifier-getter form record the same reads, uncertain accesses,
writes, and `assignment_only` (see [gotchas](./gotchas.md#same-file-helper-follow-dual-path)).

### Tracer plugins (`vue_vet_plugins`)

| Concern | Location |
| --- | --- |
| Engine types / empty default catalog | `vue_vet_reactivity` (`NamedApiBag`, `TracerPlugin`, `TraceConfig`) |
| Ecosystem hardcode (Nuxt data bags, vue-i18n `useI18n` ambient-on-call) | **published** `vue_vet_plugins` |
| Auto-load | `vue_vet_oxc`, `vue_vet_project` (`ensure_default_plugins`), `vue_vet_session` |
| crates.io order | `core` → `reactivity` → `plugins` |

See [vue_vet_plugins README](../../crates/vue_vet_plugins/README.md) and
[architecture](./architecture.md#reactivity-tracer-plugins-vue_vet_plugins).

### ExportState policy algebra (A6 linking)

Cross-module seeds cross only **finished** export states. Phase one builds
per-module `locals: name → ExportState`; the link-time fixed point refines
forwards and publishes seedable states. Executable checks live in
`crates/vue_vet_reactivity/src/trace/summary/export_lattice.rs` (pure, no
AST); `summary/resolve/worklist.rs` is the impure adapter (facts, links,
fixed-point queue). Module layout is listed in
[`crates/vue_vet_reactivity/README.md`](../../crates/vue_vet_reactivity/README.md#module-layout).

| State | Seedable? | Meaning (under-approx) |
| --- | --- | --- |
| `Known(k)` | yes | Value is already a reactive binding of kind `k` |
| `Factory(k)` | yes | Call returns scalar reactive of kind `k` |
| `Composable(shape)` | yes | Call returns object bag (fields / open spread / pending) |
| `ValueBag` / `ValueFactory` | yes | Nested method bag |
| `ComponentFactory` | yes | Setup-forward `defineComponent` wrapper |
| `ForwardReturn(name)` | no (provisional) | Body / `typeof` / `return local = call()` → resolve `name` then re-enter |
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
arms are ref-like call results → `Known(k)` (mixed plain arms quiet). Ref-like
kinds live on `ReactiveBindingKind::is_ref_like` (core); same kind keeps it,
distinct ref-like kinds merge to `Ref`.

**Seed materialize** only acts on seedable export states (`is_seedable`) —
never invent consumer bindings.

**Pending bag fields**: `const { a } = useX(); return { b: a }` records pending
`(export_key=b, root=useX, path=[], field=a)`; empty `path` means resolve the
`Composable` field on `root` (member paths still ValueBag walk).

**Publish barrier** (seed map only accepts seedable states):

1. Non-seedable → drop.
2. First publish of a name → insert.
3. Same-class bag refinement (`ValueFactory` / `ValueBag` / `Composable`) → replace.
4. Conflicting seedable classes → sticky `Ambiguous`.
5. Already `Ambiguous` → unchanged.

Axes A0–A7 can be **complete** while this algebra still gains contract
refinements — refinements bump `REACTIVITY_GRAPH_VERSION` / project
`CONVENTIONS_VERSION`, not a new axis.

## Next tracer work needs evidence first

**Do not** auto-continue pure extracts, reference-corpus KPI chasing, or a11y
as tracer A0–A7 work. A0–A7 complete is a valid resting point.

1. **Contract refinement** — an invented Conditional, a blocked seed, or a
   dual-path inconsistency found in a real component → fix + unit / oracle
   case + `REACTIVITY_GRAPH_VERSION` bump + update this record. Ranked
   candidates live in the [science memo](./research/reactivity-tracer-science.md).
2. **Consumer surface** — rules / explain / TUI / VS Code using facts already
   on the graph (product polish, not axis work).
3. **Otherwise stop.**

### Out of scope / A0 stop (never blocks complete)

| Gap | Why |
| --- | --- |
| Further A4 control-flow depth | Already deep; wrong axis for recall |
| Whole-program JS soundness / full alias analysis | Charter: under-approx Vue tracking only |
| App Tree provide/inject | Unique-key index is the in-scope model |
| Long-tail reactivity APIs with no analyzable return | Quiet failure; prefer Factory return-kind analysis (body / `.d.ts`) over name allowlists; expand an allowlist only with oracle evidence |
| Inventing nested keys for deep `watch(reactive)` | Violates under-approx; deep root `*` is the contract |

### Charter invariants (must not regress)

1. **Under-approx:** invented *concrete* property keys are bugs; missing edges
   are acceptable quiet failure. Deep-watch root `property: "*"` is an
   explicit, oracle-aligned sentinel.
2. **No runtime execution** as the product engine.
3. **Symbol identity** for cross-module linking; bare names are not enough.
4. **Dual-path parity** between inline and helper-backed / peeled forms.

## Prior art (verified)

There is no official Vue "reactivity analysis plugin" that builds a static
dependency graph. `eslint-plugin-vue` reactivity-loss rules are shallow AST
patterns (no edge set); the Vapor Mode compiler computes static deps for
codegen, not a public IR; Vue DevTools is a runtime graph (oracle ground truth,
not lint). A **serializable static reactivity graph library** is the gap. Vue
3.6 / alien-signals rewrites raise the value of the runtime oracle as both
precision ruler and version-compat net (`pauseTracking` etc. stay
capability-gated).

## History

Per-version contract refinements (v22–v41), the 2026-07 reorientation from A4
depth to A1/A3 breadth, and the dated decision log live in `git log` for this
file and in the tracker issues; they are not restated here.
