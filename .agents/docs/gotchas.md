# Known gotchas

## Vize API churn is expected

Vize is not yet production-stable and publishes frequently. Keep the dependency exact-pinned. An upgrade is a compatibility task: compile, inspect API changes, run golden fixtures and diagnostic snapshots, and record behavior differences. Do not change the version range just to unblock dependency resolution.

Vize 0.291.0 requires Rust 1.95 or newer, and its Oxc dependency requires Rust 1.93 or newer. The original Rust 1.85 pin therefore failed before Vue Vet compiled. Keep the exact Rust toolchain aligned with the workspace `rust-version`, regenerate `Cargo.lock` only with that toolchain, and validate upgrades through `just roll-rust`.

## SFC offsets are not plain string positions

Vize block locations are offsets into the original SFC, while downstream parsers may operate on extracted script or template content. Every extraction needs an explicit offset map back to the original source. Unicode makes byte/character confusion visible; CRLF makes line calculations visible.

The template parser reports offsets relative to the extracted template content.
The Vize adapter must add the SFC template block's start offset exactly once
before creating Vue Vet spans. Persist full diagnostic snapshots for ASCII,
Unicode, multiline, and multiple-directive inputs whenever this adapter changes.

## A parsed SFC contains several language surfaces

Template, ordinary script, script setup, styles, and custom blocks have different grammars and semantics. Never run one generic pattern language over the full `.vue` file and present the result as semantic certainty.

Oxc spans are relative to the extracted script block. Add the Vize
`SfcScriptBlock.loc.start` offset exactly once before deriving line and column.
Ordinary script and script setup remain separate fact blocks so duplicate names
and future merge semantics are explicit rather than accidental.

## Configuration is part of diagnostic identity

Preset expansion happens before explicit rule overrides. Path globs normalize
separators before matching, severity overrides run before exit-policy scoring,
and suppressions run after diagnostics exist so unused directives can be
reported. Future cache keys must include the serialized effective configuration.

## Do not add a parallel pattern engine

Structural patterns can rediscover problems already proven by Vize/Oxc-backed
rules while adding a second parser, offset model, confidence model, and
dependency lifecycle. Keep built-in analysis on the semantic stack. A future
customization mechanism needs demonstrated user demand and a new product
decision rather than being introduced as an implementation shortcut.

## Diff mode is a graph problem

A changed file can introduce a diagnostic whose best source location is in another file. Changed-line filtering must track causality through the project graph rather than dropping every finding outside the textual diff.

Project resolution is deliberately smaller than Node, TypeScript, Vite, and
Nuxt resolution. Missing relative or supported alias targets are diagnostics;
package imports are external graph nodes; unsupported `#` aliases remain
visible. Never silently reinterpret an unresolved edge as an external package.

Cache corruption is a miss, not a scan failure. Cache keys must change when any
normalized semantic input changes; raw dependency ASTs must never be persisted.
Diff filtering keeps every project diagnostic because removing an edge can
create a finding on a file that has no remaining path back to the changed file.

## The current score is provisional

The initial score is a deterministic vertical-slice implementation, not yet a stable measure of repository health. Before external release, scoring needs documented weights, stable diagnostic fingerprints, baselines, and reference-corpus evidence. Do not optimize rules around the current arithmetic.

## Vue behavior must be capability-gated

Vue compiler behavior is not uniform across Vue 3 releases. Reactive props
destructuring and `useTemplateRef()` are available in Vue 3.5+, while direct
`defineProps()` destructuring loses reactivity in 3.4 and older. Version-aware
rules read the nearest numeric `vue` requirement from `package.json`, include
that manifest in cache inputs, and stay quiet when the capability is unknown.

Tracking scopes (`watchEffect*`, `computed`, `watch` sources) subscribe only to
reactive reads reached during synchronous execution. Model guarded reads as graph
edges derived from Oxc control structure; do not rediscover them with source text
matching. A conditional edge is evidence about tracking behavior, so diagnostics
must describe the condition and recommend explicit `watch` sources when all
inputs are intended to invalidate the effect. Keep every direct read occurrence
in the graph: consumers need earlier unconditional reads to suppress false
positives. Reads after a top-level `await` are `AfterAwait` because Vue stops
dependency collection at that synchronous boundary. Deferred callbacks
(`then` / `nextTick` / …) are `OutsideTracking` rather than silent drops.
Arbitrary nested callbacks, local lookalike functions, and write-only assignment
targets remain outside parent-scope tracking. See
[reactivity tracer](./reactivity-tracer.md).

## Cross-module reactivity is a summary problem

Do not concatenate files and parse the result as one script. The reactivity
linker analyzes each module separately, consumes only project-resolved edges,
and propagates Vue Vet-owned summaries through named/default exports, barrels,
multi-hop re-exports, and cycles. Exported composables are summarized only when
a named function returns a statically keyed object whose values resolve to
proven local reactive bindings; consumers are seeded only for direct object
destructuring of a symbol-resolved imported call.

Local variable names are never enough for module propagation. Export collection,
composable returns, imported calls, and effect reads must agree on Oxc symbol
identity so shadowed parameters and function-local refs do not leak across the
module boundary. Conflicting star exports, ambiguous links, unresolved imports,
dynamic keys, namespace consumers, and unsupported return shapes stay quiet
instead of inventing certainty. Standalone JavaScript/TypeScript files are wired
into the project graph today. Template→script join is **not** blocked on Vize:
`vize_atelier_sfc` already gives absolute block `loc` offsets, and
`vize_atelier_core` parse trees expose `Interpolation`, directive `exp`/`arg`,
and `ExpressionNode::loc()`. The historical gap was vue-vet under-extraction
(elements-only walk, directive-name spans, no interpolation surfaces). Today
`TemplateFacts.expressions` carries those Vize surfaces with SFC-absolute spans
and `join_template_reads` prefers them. Identifier reads are filled by Oxc
(`vue-vet-oxc::template_expression_identifiers`) so static member properties are
not mistaken for bindings; lexical scan is only the empty-list fallback. Handler free-vars and template-local `v-for` / `v-slot` aliases are filtered at
extract time. `TemplateExpressionFact.identifiers` is `Some(…)` when resolved
(including empty = no free reads); only `None` triggers the lexical join
fallback—do not treat empty `Some` as unknown.

Cross-file module tracing for `.vue` uses the preferred script block
(`script setup` first) as `ModuleSource::sfc_script` with Vize `loc.start` and
the full SFC as `span_source`. Standalone JS/TS modules keep offset 0. Seed
spans must use the same origin/offset as module re-trace (`source_offset` +
`span_source`), or `reference_resolves_to_binding` will drop composable reads.
After seed linking, project graph re-runs `join_template_reads`. The CLI then
applies that module graph onto SFC facts and runs rules, so composable seeds
affect per-file diagnostics—not only `module_reactivity` debug output.

Content cache keys include `CACHE_FORMAT_VERSION`, ruleset version, and
`REACTIVITY_GRAPH_VERSION`; bump those when analysis behavior changes so local
caches do not serve stale graphs. Dual ordinary+setup blocks are not merged
into one module.

## EffectScope `.run` requires provenance

Only `const scope = effectScope(); scope.run(cb)` is a tracking-scope body.
Arbitrary objects with a `.run` method must stay quiet — inventing
`effectScope.run` edges violates under-approx. See the reorientation in
[reactivity tracer](./reactivity-tracer.md).

## Runtime oracle is the precision ruler

Committed `crates/vue-vet-reactivity/oracle/expected/*.json` capture Vue
`onTrack` deps. Static tests must keep **tracer tracking-reads ⊆ runtime deps**.
Refresh with `just oracle-refresh` (Node + pnpm) when Vue tracking semantics
change (including alien-signals / 3.6). Do not treat the 280 syntax corpus as
recall evidence.

`pauseTracking` / `enableTracking` are **not** public `vue` package exports in
3.5.x; the oracle harness imports them from `@vue/reactivity` while static
sources may still name them under `from 'vue'` (matching common docs / Nuxt
re-exports). Treat that API surface as capability-gated, not guaranteed.

## Sync HOF callbacks still track

`list.value.filter(x => query.value)` runs the callback during the parent
tracking flush. Nested arrows that are arguments to known sync Array methods
stay inside the parent scope; deferred containers (`then` / `nextTick` / …)
remain outside.

## Ecosystem APIs beyond `vue`

`storeToRefs` (pinia / `#imports`) and `useRoute` / `useRouter` (vue-router /
`#imports`) are allowlisted reactivity sources. Unknown package callees stay
quiet. Do not treat every `use*` auto-import as reactive without evidence.

## Edge `from` labels (graph v4)

Computed edges prefer the assigned binding name (`doubled`). Other scopes use
`{kind}:{callee}@{offset}`. Template joins use `template:{surface}@{offset}` so
multiple interpolations do not collapse. `to` remains a bare binding name for
consumer matching until symbol/module IDs land.

## Instance seeds are bags, not field injections

`const bag = useComposable()` records `bag` under `composable_instances` so
`bag.field.value` can resolve. Do **not** also push each shape field as a
top-level `ReactiveBindingFact` — that invents edges for bare `field.value`
when the consumer never destructured. Destructured calls
(`const { field } = useX()`) remain the only path that seeds a local `field`.
