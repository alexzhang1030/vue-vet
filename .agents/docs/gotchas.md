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

## Package directories can look like source files

npm/pnpm install trees often contain directories (or directory symlinks) named
like packages with a `.js` suffix, for example `node_modules/pixi.js`.
`Path::extension` reports `js`, and `DirEntry::file_type` may describe a
symlink rather than a directory, so naive walks try to `fs::read` them and
fail with EISDIR. Project walks must skip `node_modules` entirely and only
accept paths where `Path::is_file()` is true after symlink resolution.

## Safe fixes need complete source coverage

A diagnostic span is not automatically a safe replacement span. For example,
the template fact for `autofocus` precisely covers its name but not a possible
value. The first safe producer therefore removes only boolean `autofocus` and
leaves `autofocus="..."` as a visible manual finding. Never turn a name-only span
into a partial edit that leaves invalid syntax behind.

Fix ranges are original-source UTF-8 byte ranges. Validate both endpoints before
editing, apply multiple ranges from the end of the source, and preserve all
untouched bytes so Unicode and CRLF remain unchanged. The current executor is
deliberately single-file and rejects a multi-file plan before any write; do not
weaken that failure until issue #9 adds a real cross-file transaction and
rollback protocol. Atomic replacement preserves the intended file contents, not
all timestamps, ACLs, extended attributes, or platform metadata; keep that
limitation visible until the project defines and tests a metadata policy.

## Do not add a parallel pattern engine

Structural patterns can rediscover problems already proven by Vize/Oxc-backed
rules while adding a second parser, offset model, confidence model, and
dependency lifecycle. Keep built-in analysis on the semantic stack. A future
customization mechanism needs demonstrated user demand and a new product
decision rather than being introduced as an implementation shortcut.

## Diff mode is a graph problem

A changed file can introduce a diagnostic whose best source location is in another file. Changed-line filtering must track causality through the project graph rather than dropping every finding outside the textual diff.

Project import resolution uses `oxc_resolver` (Rolldown / enhanced-resolve), not
a hand-rolled path matcher. Successful resolves into `node_modules` or outside
the scanned file set are external graph nodes; only true resolve failures raise
`unresolved-import`. Vue Vet still does **not** execute `vite.config.*` /
`nuxt.config.*` — aliases come from Vite defaults (`@` → `src`, `~` → root),
tsconfig paths (including `.nuxt/tsconfig.json`), and package `exports`.
Never silently reinterpret an unresolved edge as an external package.
`oxc_resolver` is pinned to `11.21.0` because `11.22+` requires `dashmap 6.2.1`
while Vize pins `dashmap =6.1.0`. Always absolutize/canonicalize the scan root
before building the resolver: `vue-vet .` must not leave alias targets as `"."`,
or Nuxt `~/…` imports fail even when the files exist. On Windows, also strip
compatible `\\?\` verbatim prefixes after canonicalize — otherwise alias targets
and `Path::strip_prefix` disagree with `oxc_resolver`'s ordinary `C:\…` paths and
`@/` / `~/` imports look unresolved in CI.

Nuxt component auto-imports do not use the raw file stem. `HeroDemo.client.vue`
is registered as `HeroDemo` (and `LazyHeroDemo`); nesting and `index.vue` also
change the PascalCase name. Matching only `file_stem` produces false
`unused-component` warnings. Prefer convention naming, and when present read
`.nuxt/components.d.ts` / `.nuxt/types/components.d.ts` instead of executing
`nuxt.config`.

Cache corruption is a miss, not a scan failure. Cache keys must change when any
normalized semantic input changes; raw dependency ASTs must never be persisted.
Diff filtering keeps every project diagnostic because removing an edge can
create a finding on a file that has no remaining path back to the changed file.

## The current score is provisional

Scoring is deterministic but still a product experiment, not a stable health
certificate. Severity weights remain Error 10 / Warning 3 / Info 1. The public
score is **density-normalized by scanned files** (Sonar/CodeClimate debt-ratio
and StackHealth lint-density style), not absolute finding count:
`score = floor(100 × capacity / (capacity + raw))` with
`capacity = max(files_scanned, 1) × 50`. Sparse warnings in a large Nuxt app
stay high; the same absolute count concentrated in a tiny project drops harder.
Before external release, keep documenting weights, fingerprints, baselines, and
corpus evidence. Do not tune rules merely to chase the current number.

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
(`vue_vet_oxc::template_expression_identifiers`) so static member properties are
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
caches do not serve stale graphs. Dual ordinary+setup blocks re-trace as setup
plus `{path}#script` (not a single concatenated module).

## Performance: do not re-serialize the hot path

CLI scan follows oxlint's model (parallel files, sequential seed barrier). Do
not force single-threaded analysis without a determinism bug, and do not clone
whole project graphs into every rule when a reference or scoped apply suffices.
Module re-parse for seed re-trace is accepted cost until a shared semantic arena
exists; optimize by parallelism first, not by inventing a second IR.

## EffectScope `.run` requires provenance

Only `const scope = effectScope(); scope.run(cb)` is a tracking-scope body.
Arbitrary objects with a `.run` method must stay quiet — inventing
`effectScope.run` edges violates under-approx. See the reorientation in
[reactivity tracer](./reactivity-tracer.md).

## Project graph node ids are not module ids

`ProjectGraph` edges use `file:{path}` node ids. Reactivity module graphs and
template maps use bare logical paths (`Parent.vue`). Prop-flow joins (and any
future edge→module joins) must strip the `file:` prefix before looking up
templates or `module_reactivity` entries — otherwise sites silently vanish.
Structural `component_nav` already normalizes; do not copy raw `edge.from` /
`edge.to` into module-id APIs.

Quiet gaps still expected after fixture sweeps: whole-object `v-bind="obj"`,
complex prop expressions, and App Tree provide/inject remain under-approx
stops (see [reactivity tracer](./reactivity-tracer.md)).

## Runtime oracle is the precision ruler

Committed `crates/vue_vet_reactivity/oracle/expected/*.json` capture Vue
`onTrack` deps. Static tests must keep **tracer tracking-reads ⊆ runtime deps**.
Refresh with `just oracle-refresh` (Node + pnpm) when Vue tracking semantics
change (including alien-signals / 3.6). Do not treat the 280 syntax corpus as
recall evidence.

`pauseTracking` / `enableTracking` are **not** public `vue` package exports in
3.5.x; the oracle harness imports them from `@vue/reactivity` while static
sources may still name them under `from 'vue'` (matching common docs / Nuxt
re-exports). Treat that API surface as capability-gated, not guaranteed.

## Watch source dep keys

`watch(ref)` / `watch([ref, …])` track each ref's **`.value`** key at runtime.
Static bare-identifier sources must record `property: Some("value")` for
ref-like bindings — a property-less read invents an onTrack identity that never
appears. Bare `watch(reactiveObj)` deep-tracks many keys; stay quiet rather than
emit a single property-less edge.

## Sync HOF callbacks still track

`list.value.filter(x => query.value)` runs the callback during the parent
tracking flush. Nested arrows that are arguments to known sync Array methods
stay inside the parent scope; deferred containers (`then` / `nextTick` / …)
remain outside.

## Ecosystem APIs beyond `vue`

`storeToRefs` (pinia / `#imports`) and `useRoute` / `useRouter` (vue-router /
`#imports`) are allowlisted reactivity sources. Unknown package callees stay
quiet. Do not treat every `use*` auto-import as reactive without evidence.

Nuxt (and unplugin-auto-import) often call `ref` / `watchEffect` with **no**
`import` statement. The tracer treats bare identifiers as Vue APIs only when
Oxc marks them unresolved and the name is on the `vue` / `#imports` allowlist.
A local `function ref()` still wins and stays quiet. Empty module facts mean
under-approx miss, not “100% reactive.”

## Edge `from` / `to_id` labels (graph v4–v6)

Computed edges prefer the assigned binding name (`doubled`). Other scopes use
`{kind}:{callee}@{offset}`. Template joins use `template:{surface}@{offset}` so
multiple interpolations do not collapse. **`to` stays a bare binding name** for
rule matching (`unused-binding` etc.). Graph **v6** adds optional
`to_id = {name}@{offset}` (read span) via `ReactiveDependencyEdge::to_identity()`.

## Dual ordinary + setup scripts

When both `<script>` and `<script setup>` exist, Vize emits:
- primary `module_source` = setup (id = file path)
- `ordinary_module_source` = ordinary (id = `{path}#script`)

Project re-traces both with seeds; CLI applies setup graph to Setup blocks and
`#script` graph to ordinary Script blocks. Prefer-setup alone dropped ordinary
seeded analysis.

## Instance seeds are bags, not field injections

`const bag = useComposable()` records `bag` under `composable_instances` so
`bag.field.value` can resolve. Do **not** also push each shape field as a
top-level `ReactiveBindingFact` — that invents edges for bare `field.value`
when the consumer never destructured. Destructured calls
(`const { field } = useX()`) remain the only path that seeds a local `field`.

The graph retains `composable_instances` (v5) so template joins can resolve pure
member chains `bag.field` / `bag.field.value` after module re-trace. Free-id
extraction only yields `bag` for those expressions; without the instance map,
template would stay quiet. Operator-bearing expressions (`bag.field + x`) stay
quiet — under-approx, not a mini expression evaluator.

Same-file `function useX()` / `const useX = () => ({ field: ref(0) })` (including
parenthesized arrow objects) also seed instance bags and destructure fields.
Function-local `const field = ref(0)` used only to build the return shape is
**not** published as a top-level binding — otherwise it collides with
`const { field } = useX()` and invents bare `field.value` edges.

Composable return-shape resolution must use the **same `script_offset`** as
binding spans. Hardcoding offset `0` makes `return { signal }` miss nested refs
inside SFC `<script setup>` (absolute spans) and silently drops same-file
instance bags — a quiet A6 failure, not under-approx by design.

Cross-module export shapes are not only `export function useX`. Also register:
- `export const useX = () => ({ … })` / `export const useX = function () { … }`
- `export default function useX() { … }` (exported name `default`, local name `useX`)
Anonymous default arrows stay quiet (no local name to hang a shape on).

## provide/inject without an App Tree

Injection is linked by a **project-wide provide index**, not a component
ancestor chain. Rules:

- Seed an `inject` local only when **exactly one** provide site for that key has
  a known reactive shape (or when a static default value has a known shape).
- **Multiple** provides of the same key stay quiet — nearest-ancestor selection
  needs an App Tree we deliberately do not build yet.
- String keys match exactly.
- **Imported** keys (`import { ThemeKey } from './keys'`) match by
  `(specifier, export name)` so shared symbol modules link across files.
- **Local** keys (`const ThemeKey = Symbol()`) use definition span identity —
  two files each defining their own `Symbol()` never cross-link.
- `provide(api)` where `api` is a composable instance bag seeds
  `composable_instances` on the inject local (not a scalar binding).
- Same-file `provide('k', useX())` also seeds when `useX` has a known return
  shape (no intermediate bag variable required). The call must **resolve to the
  composable def span** — a block-shadowed non-composable `useX` stays quiet
  (name-only matching invents outer bag fields).
- `toValue(() => …)` invokes the getter synchronously; reads inside that getter
  stay in the parent tracking scope (like Array HOF callbacks). `unref` does not
  call functions.
- Sync HOF callbacks also include **String#replace / replaceAll** replacers
  (and Array methods), plus well-known statics **`Array.from(…, mapFn)`** and
  **`JSON.parse(…, reviver)`** (receiver must be the `Array`/`JSON` identifier —
  bare `.from`/`.parse` on unknown objects stay quiet). Callback **argument
  index** is callee-specific: prototype HOF → arg 0; replace/from/parse → arg 1.
  First-arg-only forms (`Array.from(() => x)`, `JSON.parse(() => x)`,
  `str.replace(() => x)`) must stay quiet — runtime does not invoke them as
  mapFn/reviver/replacer. Deferred callbacks (`then`/`setTimeout`/`nextTick`)
  stay outside tracking.
- Factory defaults (`inject(key, () => ref(0))`) stay quiet; plain
  `inject(key, someRef)` may seed from the default.
