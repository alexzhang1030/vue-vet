# Reactivity tracer

`vue-vet-reactivity` is the Vue Vet-owned **static reactivity tracing library**.
Lint rules are the first consumer, not the capability ceiling. The goal is the
most complete static model of Vue reactivity tracking that stays high-confidence,
deterministic, and free of dependency AST leakage.

Related: [architecture](./architecture.md), [gotchas](./gotchas.md),
[literature matrix](./research/reactivity-tracer-literature.md),
[root roadmap](../../ROADMAP.md).

## Product stance

- Approximate Vue's **synchronous tracking semantics** with static facts.
  Do not execute components, effects, or Proxies.
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

## Current baseline (shipped)

| Axis | Status |
| --- | --- |
| A1 Bindings | Vue primitives + aliases/`#imports`/`defineModel`; symbol identity |
| A2 Scopes | effects + computed + watch sources/callbacks + effectScope + onScopeDispose |
| A3 Reads | Direct ref-like `.value`, reactive members, composable instance `bag.field.value` |
| A4 Conditions | if / early-exit / ternary / short-circuit / switch → guards with **roles** |
| A5 Boundaries | await / pauseTracking / deferred callbacks / watch jobs → non-tracking kinds |
| A6 Modules | composable shapes including parametric `toRef(param, key)` + instance seeds |
| A7 Contract | v3: scopes/writes/edges/template_reads + effects projection |
| Evidence | 280 corpus + unit coverage + after-await/prefer-computed rules |

## Completeness ladder

Each level is a shippable lib slice: facts + fixtures + docs. Rules may follow
in a later PR once the facts exist.

### L0 — Semantic charter

Record the static approximation of Vue tracking:

- A tracking scope runs synchronously; only reached reactive reads subscribe.
- Nested function bodies are outside the parent scope's tracking.
- Write-only assignment targets are not reads.
- Top-level `await` ends synchronous collection for that scope.
- Unknown interprocedural or dynamic shapes stay quiet.

**Exit:** this file + gotchas agree; no behavior change required.

### L1 — Tracking scopes

Recognize every first-class tracking region the lib claims to model:

| Scope kind | Status | Notes |
| --- | --- | --- |
| `watchEffect*` | shipped | projects into legacy `effects` |
| `computed` | shipped | callback body is a tracking scope |
| `watch` sources | shipped | source expressions / getters / arrays |
| `watch` callback | shipped | job body; all reads forced to `OutsideTracking` |
| `effectScope` / `.run` | shipped | grouping scope; nested effects still tracked |
| `onScopeDispose` | shipped | cleanup body; outside tracking |
| setup / render | partial | template identifier join shipped; full expression AST still future |

**Exit:** met — scopes + effects projection; unit fixtures; 280 corpus green.

### L2 — Read precision

Deepen demand-read modeling inside scopes:

- richer early-exit and `else if` / `switch` guard attachment
- guard roles (`early_exit`, `branch_test`, `short_circuit`)
- keep every occurrence (unconditional earlier reads still suppress FP)
- optional property-path depth policy (stay shallow until a consumer needs more)

**Exit:** complex corpus gains targeted cases; no SMT / full NTSCD required.

### L3 — Sync boundaries

Make non-tracking boundaries first-class:

| Boundary | Status |
| --- | --- |
| top-level `await` | shipped as `AfterAwait` |
| `then` / `catch` / `finally` callbacks | shipped as `OutsideTracking` |
| `nextTick` / `queueMicrotask` / `setTimeout` | shipped as `OutsideTracking` |
| arbitrary nested callbacks | remain excluded (under-approx) |

**Exit:** boundary kinds documented; fixtures; rules may consume later.

### L4 — Module summaries

Raise the composable/export ceiling without file concatenation:

| Capability | Status |
| --- | --- |
| named/default/star re-export fixed point | shipped |
| object-return composable shapes | shipped |
| destructured call seeds | shipped |
| member seeds `const x = useFoo(); x.a` | shipped (`bag.field.value`) |
| multi-return join when shapes agree | shipped (same field/kind kept; conflict → quiet) |
| parametric / dynamic keys | stay quiet |

**Exit:** module + real-world fixtures expand; Ambiguous/Opaque remains quiet.

### L5 — Stable graph contract

Version the public fact shape for multi-consumer use:

- explicit scope nodes and typed read/guard/seed edges (incremental evolution OK)
- graph format/version field (`REACTIVITY_GRAPH_VERSION`, currently `2`) — shipped
- deterministic ordering preserved
- consumers: rules, project graph, JSON, future LSP

**Exit:** version field + documented contract; no Oxc types cross the boundary.

### L6 — SFC / template join

Cross script and template reactive surfaces:

| Capability | Status |
| --- | --- |
| Template directive expression identifiers → script bindings | shipped (`template_reads` + edges) |
| Exact expression AST / interpolation nodes | future (richer Vize expression contract) |
| Cross-file extracted `.vue` script module offsets | blocked on Vize source/offset handoff |

## Non-goals

- Executing Vue runtime, effects, or tests to discover dependencies
- Whole-program TAJS-class abstract interpretation as the default engine
- Pattern-engine duplicate of Oxc/Vize semantics
- Inventing edges for unresolved, dynamic, or conflicting exports
- Glitch-freedom scheduling (runtime concern; not a lint graph duty)

## Default delivery order

```text
L0 charter (docs)
  → L1 scopes (computed + watch sources)
  → L2 read/guard precision
  → L3 boundaries
  → L4 module summary upgrades
  → L5 contract versioning (can start lightly in L1)
  → L6 SFC join when Vize contract exists
```

Rules land after the facts they need. Prefer tracer-only PRs when the slice is
large; attach one vertical rule when it proves the slice.

## Evolution wave (landed 2026-07-24)

Shipped together as one tracer evolution:

1. **Scope-centric IR** — `TrackingScopeFact` + `scopes`; `effects` projected from effect-family scopes.
2. **Scope coverage** — `watchEffect*` / `computed` / `watch` sources.
3. **Guard roles** — early-exit / branch / short-circuit / switch discriminant.
4. **Boundaries** — `AfterAwait` + `OutsideTracking` for deferred callbacks.
5. **Module seeds** — destructure + `const bag = useX(); bag.field.value`.

Still open: full template expression AST (beyond identifier scan), extracted
`.vue` script module offset contract, pauseTracking nested in branches edge cases.

## Evolution wave 2 (landed 2026-07-25)

1. **WatchCallback** scopes — watch job bodies modeled; reads are `OutsideTracking`.
2. **Graph version** — `ReactivityGraph.version` / `REACTIVITY_GRAPH_VERSION` (now 3).
3. **Rule** — `vue-vet/reactivity/no-after-await-watch-effect-dependency`.

## Evolution wave 3 (landed 2026-07-25)

1. **Scope writes** — `ReactiveWriteFact` + `assignment_only` on tracking scopes.
2. **Rule** — `vue-vet/reactivity/prefer-computed`.

## Evolution wave 4 (landed 2026-07-25)

1. **effectScope / onScopeDispose / pauseTracking** boundaries.
2. **Parametric composable** `toRef(param, key)` / param pass-through shapes.
3. **Template join** — identifier scan from directive expressions onto bindings.
4. **Dependency edges** — computed/effect/template inverted depends-on list.

## Decision log

| Date | Decision | Notes |
| --- | --- | --- |
| 2026-07-24 | Lib-first completeness over rule-only ROI | Tracer is an ecosystem library; lint is first consumer |
| 2026-07-24 | Static approximation only | Runtime is the semantic reference, never the execution mode |
| 2026-07-24 | Under-approx + quiet failure remains default | Completeness does not mean guessing |
| 2026-07-24 | Full evolution wave E1–E4 | scopes + guards + boundaries + module member seeds |
| 2026-07-25 | Wave 2: WatchCallback + graph version + after-await rule | Vertical slice proving async-boundary facts |
