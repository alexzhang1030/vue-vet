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
| A2 Scopes | `watchEffect*` + `computed` + `watch` sources (`TrackingScopeFact`) |
| A3 Reads | Direct ref-like `.value`, reactive members, composable instance `bag.field.value` |
| A4 Conditions | if / early-return / ternary / short-circuit / switch → guards with **roles** |
| A5 Boundaries | top-level `await` → `AfterAwait`; `then`/`catch`/`nextTick`/… → `OutsideTracking` |
| A6 Modules | export fixed point; composable shapes; destructure **and** instance member seeds |
| A7 Contract | `ReactivityGraph { bindings, scopes, effects }`; effects projected from effect-family scopes |
| Evidence | 280 corpus fixtures + unit coverage for scopes / boundaries / member seeds |

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
| `watch` callback | later | optional; often non-tracking relative to sources |
| `effectScope` hooks | later | pause/resume boundaries |
| setup / render | blocked | needs template + script join via Vize contract |

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
- graph format/version field when the wire shape breaks
- deterministic ordering preserved
- consumers: rules, project graph, JSON, future LSP

**Exit:** documented contract + snapshot gates; no Oxc types cross the boundary.

### L6 — SFC / template join

Cross script and template with exact spans. Blocked on Vize-owned
source/offset handoff for extracted script blocks (see gotchas).

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

Still open: watch callback modeling, effectScope, parametric composables, SFC/template join (L6), graph format version field when wire consumers need it.

## Decision log

| Date | Decision | Notes |
| --- | --- | --- |
| 2026-07-24 | Lib-first completeness over rule-only ROI | Tracer is an ecosystem library; lint is first consumer |
| 2026-07-24 | Static approximation only | Runtime is the semantic reference, never the execution mode |
| 2026-07-24 | Under-approx + quiet failure remains default | Completeness does not mean guessing |
| 2026-07-24 | Full evolution wave E1–E4 | scopes + guards + boundaries + module member seeds |
