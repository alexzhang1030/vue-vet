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

## crates.io API calls need a User-Agent

The crates.io HTTP API returns **403** for clients that omit a descriptive
`User-Agent` (including bare `curl -fsS`). Release waits that poll
`/api/v1/crates/<name>/<version>` after `cargo publish` must send one (see
`.github/workflows/release.yml`). A 403 is not “not indexed yet”; treating it
as missing visibility aborts after `vue_vet_core` uploaded and skips
`vue_vet_reactivity`, GitHub Release, and npm. Re-runs must skip versions
already on the registry (`cargo publish` refuses duplicates).

## `has_children` is not accessible content

Template facts keep `has_children` for structural rules (`valid-v-html`,
`valid-v-text`). Accessibility rules that need a discernable name
(`anchor-has-content`, `button-has-content`, `heading-has-content`) must use
`has_accessible_content`: non-whitespace text, interpolation, `v-text` /
`v-html`, or a descendant `img`/`area` with non-empty `alt`. Icon-only
element trees (for example `<a><div class="i-carbon-logo-github" /></a>`)
set `has_children` and clear `has_accessible_content`. Do not treat child
presence alone as an accessible name; `title` is also insufficient (a static
`title` may only contribute a safe `aria-label` insert preview).

## Template element spans are start-tag only

Vize `ElementNode` locations used for `TemplateElementFact.span` cover the
opening tag, not the full element including children. Nested-structure rules
must not use span containment — compute descendant facts while the tree is
still available (`has_labelable_descendant` for `label-has-for`,
`has_label_ancestor` for `form-control-has-label`).

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
`unresolved-import`. A small allowlist is classified as external **before**
resolve (`node:` / `nodejs:`, stylesheets, `virtual:…`, `uno.css`,
`*/auto-routes`, `#imports`) so Vite/Nuxt virtual and non-JS imports do not
flood real apps — see [project graph](../../docs/project-graph.md). Do **not**
reinterpret arbitrary failed resolves as external packages. Vue Vet still does
**not** execute `vite.config.*` / `nuxt.config.*` — aliases come from Vite
defaults (`@` → `src`, `~` → root), tsconfig paths (including
`.nuxt/tsconfig.json`), and package `exports`.
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
multi-hop re-exports, and cycles. Exported composables are summarized when:

- a named function returns a **statically keyed object** whose values resolve to
  proven local reactive bindings (`ExportState::Composable`) — consumers seed
  via destructuring or instance bags; or
- every analyzable return is the **same scalar reactive kind** (`return ref(0)`,
  `return flag`, or a declared `.d.ts` return type `Ref` / `ComputedRef` / …)
  → `ExportState::Factory(kind)`, and `const x = useX()` seeds a local binding
  of that kind (the imported function name itself is never a Ref); or
- a declared `.d.ts` / annotated return type is an **object type literal** (or a
  same-file `interface` / `type` alias of one) whose static properties resolve to
  ref-like types → `ExportState::Composable(shape)` even when the body is only
  `declare function` (VueUse `useElementSize(): { width: Ref; height: Ref }`).
  Non-reactive fields (`stop: () => void`) stay out of the shape.

Mixed object/scalar returns, conflicting kinds, and unanalyzable returns stay
quiet. Plain `reactive()` returns typed as interfaces of string/boolean fields
(Nuxt `useColorMode(): ColorModeInstance`) still stay quiet — there is no
Ref-shaped evidence in the type surface.

Declared object-bag shape helpers must stay off the `const x = ref(0)` cold
path (`trace_1k_modules`): build the return-statement index only after seeing a
function/arrow init, compute `.d.ts` shapes lazily when body analysis returns
`None`, and keep shape helpers `#[inline(never)]` so they do not bloat export
collection instruction cache.

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

CLI scan follows oxlint's model (parallel files, coordinated seed resolution).
Never restore one scoped native thread per module: Oxc semantics are not `Send`,
and parking thousands of sticky workers exhausts stacks and defeats
`--threads`. `TraceModulesOptions::max_workers` bounds both phases. The Oxc
adapter supplies prepared Vue Vet-owned phase-one facts from its file parse;
unseeded modules reuse that graph. Seeded consumers reparse only when source or
seed plans change; unchanged final graphs are retained by `ModuleTraceState`.
Module failures are collected independently so healthy links still resolve.
The multi-sample 1k/5k synthetic module benchmark guards this scaling model.

Cache lookup and cache-miss analysis must share `WorkspaceInputSnapshot`; do not
add a pre-hash walk that rereads the same files. Per-file package capabilities
come from `PackageIndex`, not repeated ancestor I/O. Long-lived sessions retain
source bytes, Nuxt declaration mappings, facts, raw file diagnostics, per-file
structural graph partitions, module plans/graphs, and reverse dependencies.
`apply_changes` updates exact paths in that snapshot; an edit must not trigger a
fresh workspace walk or rebuild unrelated structural partitions. First discovery
must also merge overlay-only paths that the filesystem walk never saw (unsaved
new buffers). `WorkspaceInputSnapshot::apply_changes` is strongly exception-safe:
on `Err`, the retained snapshot is unchanged. Session `apply_changes` is
transactional over overlays + snapshot + revision.

The session revision cannot live in a separate atomic publication step from
input mutation. Otherwise an analysis may observe the old revision after the
new bytes are installed and commit stale state. Revision, retained input, and
committed analysis share one `SessionCore` lock; CPU work uses captured `Arc`
snapshots outside the lock and publishes only if the captured revision still
matches. Keep the barrier regression test when changing this lifecycle.

Resolver inputs are semantic invalidation, not only structural-cache inputs.
`ProjectContext.epochs` stores independent counters per `ContextChangeKind` so
consecutive mutations before `analyze_affected` cannot drop an earlier kind.
Package manifests participate in module resolution (`imports`/`exports`/…), so
they invalidate all source consumers — not only `RuleEnvironment` capability
keys. File-rule caches also compare the consumed final module graphs before
reuse. Incremental-vs-clean tests cover package capability, package imports,
tsconfig, lockfile, Nuxt declarations, and consecutive mixed context mutations.

`TraceModulesOptions::max_workers` must install a dedicated pool for public
callers. Only session analysis sets `reuse_current_pool: true` after installing
its outer `--threads` pool; never ignore `max_workers` for the standalone API.

File-rule diagnostic reuse must compare a `FileRuleInputKey` covering source,
`RuleEnvironment`, and the final primary/ordinary `ReactivityGraph`s. Never
reuse diagnostics from source+environment equality alone when the linked module
graph may have changed. Do not `serde_digest` full graphs on the hot path —
prefer Arc content equality (or a future trace-time digest). Do not invent a
unified template/script AST IR; keep `ModuleSummary` as the cross-file semantic
boundary.

Never discard the dirty `FileId` set returned by
`WorkspaceInputSnapshot::apply_changes`. Session analysis must schedule from
`PendingChanges` via `ChangeImpact` / `DirtyPlan`. Cancellation must not clear
pending dirty state. A no-op `analyze_affected` when the revision is unchanged
must return the last snapshot without re-entering the pipeline.

**Dirty `FileId` ≠ dirty work.** A small `affected_files()` set only proves parse
scheduling was narrow. Phase-one may still visit every module summary (cheap when
already attached). Prove locality with work counters (`files_parsed`,
`seed_plans_recomputed`, `export_resolve_ran`, layered rebuild, COW clones,
rules rerun), not with set size alone.

**Linking surface ≠ `ModuleSummary` equality.** Export/seed reuse keys on
imports/exports/locals/provides/injects + links. A leaf body edit that only
changes `local_graph` must not force `resolve_exports`. Do not key linking
cache on full `ModuleSummary` (it includes the local graph). Never rebuild a
cloned `LinkingSurface` map for every module on each scan — retain
`Arc<ModuleSummary>` and prefer `Arc::ptr_eq`, then compare linking fields in
place. O(N) deep clones on cold `trace_modules` / independent leaf edits are a
known CodSpeed regression.

**Template/prop layers must not `make_mut` reused base graphs on warm scans.**
Keep base reactivity from module-trace separate from the layered final graphs;
reuse the layered `Arc<[ModuleReactivity]>` when base graph pointers and
`SfcFacts` pointers are unchanged.

**`ModuleSource` equality ignores `span_source`.** Style-only SFC edits change
the wrapper file bytes without invalidating script body IR when `source` +
`source_offset` match. Do not reintroduce `span_source` into `PartialEq`.

**SFC block reuse keys on content digest + absolute loc.** If a preceding block
grows/shrinks, later blocks' `start`/`end` change and must rebuild even when
their text is identical. Style-only edits after other blocks are the common
full-reuse path.

**Context invalidation ≠ re-parse.** Epoch bumps for tsconfig, lockfile,
package resolution, Nuxt declarations, or source membership must refresh
resolution / environment / indexes / rules as needed. They must not force
`analyze_candidate()` on unchanged source bytes. Prefer `ChangeImpact` domains
over a boolean `invalidate_all_sources`.

**Prefer internal Arc partitions over a top-level `Arc<ProjectGraphState>`.**
Session state holds `ProjectGraphState` by value; `structural` and
`module_trace` are independently `Arc`-shared and copy-on-write. Count
`partition_cow_clones` / `graph_cow_clones`. Do not reintroduce a single outer
Arc that `make_mut`s the entire linking state.

**No-op / product publish must stay refcount-only.** `AnalysisSnapshot` keeps
`summary` / `graph` / `coverage` / `issues` / `analyzed_files` behind `Arc`.
Never reintroduce owned `Vec` fields that `Clone` deep-copies on noop.

Export resolution must not clone the entire resolved-export map each fixed-point
round — use a worklist over reverse re-export users.

Deep `.clone()` of reactivity graphs, analyzed candidates, or workspace snapshots
is a regress on the incremental path. Share with `Arc` / `ProjectGraphState::share`,
mutate with `Arc::make_mut` on the smallest partition, and restore cache hits with
`AnalysisState::share_from`. Prefer `Arc::clone` (refcount) over `T::clone` of
owned maps/vecs. Session overlay updates must not double-clone
`WorkspaceInputSnapshot` (fork once via `Arc::make_mut`, then
`apply_changes_in_place`). Reporter/CLI boundaries may `to_vec()` once when
leaving the session.

Never build the session Rayon pool in `ProjectSession::open` — warm disk-cache
hits must not pay thread-pool construction. Lazily init on the first real scan.
Never eagerly re-scan on a disk-cache hit to hydrate IR: that turns
`scan_warm_*` / CLI warm re-scans into full analyzes. Keep publishing the
cached summary/graph as `"hit"`. Empty IR is seeded on the first dirty analyze
via `force_full_parse` when `!has_file_facts()`.

**`SourceContext::new(&str)` copies the buffer.** Use it when the caller already
owns / wants to own the text (LSP documents). Hot analysis entry points that
only need positions should install `Arc<LineIndex>` without re-allocating the
source string — otherwise cold `trace_1k_*` / SFC benches regress.

**One-shot `trace_modules_with_options` must not archive linking state.** Set
`persist_linking_cache = false` (forced by that API). Archiving sorted links +
seed-plan maps that are immediately dropped regresses CodSpeed `trace_*`.
Build `returns_by_function` lazily — only after a real function/composable
candidate is found.

**Phase-two must not Rayon-schedule immediate reuse.** On persistent scans,
split reused vs dirty modules before `par_iter`. Independent leaf edits with
many reusable graphs must not pay worker scheduling for no-op reuse. Never clone
all `ModuleSource` values into a side cache map for phase-one — borrow
`state.entries` instead.

**First persistent scan must build seed plans once.** Cold session analyzes
(`scan_overlay_*`, first `analyze`) should use oneshot-style plan construction
and archive the linking snapshot after phase two — never build a plan map and
then clone every plan into work.
`AnalysisSnapshot` keeps `summary`/`graph` behind `Arc` so commit/`last_snapshot`
is refcount-only for those fields.

LSP positions are UTF-16 code units via `vue_vet_core::LineIndex`. Never publish
byte columns to the editor. Document identity must go through
`ProjectSession::file_id_for_path` rather than ad-hoc `strip_prefix`.

## Paths are identities, not suffixes

Discovery is the only boundary that converts `PhysicalPath` to normalized,
workspace-relative `FileId`. Diagnostics, edits, graph nodes, fingerprints,
cache/diff inputs, LSP, and reporters compare `FileId` exactly. Never use
`ends_with` to reconcile paths: `apps/admin/src/App.vue` and
`apps/customer/src/App.vue` are both valid and suffix matching can select the
wrong file. Reports keep analyzed source coverage separate from package,
lockfile, and tsconfig invalidation inputs.

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
computed / bracket / call prop expressions, and App Tree provide/inject remain
under-approx stops (see [reactivity tracer](./reactivity-tracer.md)). Static
prop flow joins bare identifiers, `ident.value`, static member chains
(`ident.member`, `ident.a.b`, `ident?.a?.b`), and `v-model` → `modelValue`
(root binding only; no nested key invention).

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
quiet **unless** project resolution finds a concrete file and the reactivity
linker can summarize a `Factory` / `Composable` export from that file (or a
companion `.d.ts`). Prefer return-kind analysis over growing the name allowlist.
Do not treat every `use*` auto-import as reactive without evidence. `#imports`
virtual modules still have no file body and stay quiet.

**Absence rules** (`no-computed-without-dependency`, `no-effect-write-without-read`,
`no-empty-watch-sources`, `no-watch-callback-as-tracking-scope`) must try hard
evidence first (bindings, Factory returns, aliases, classified reads). Only when
reads stay empty do they consult `uncertain_accesses` (reactivity-shaped
`.value` / `unref` / `toValue` / bare watch sources that could not be classified)
and report with `(maybe: …)`. Do not invent edges; do not treat empty reads as
ironclad proof when soft evidence remains.

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

`const bag = useComposable()` for an **object** composable records `bag` under
`composable_instances` so `bag.field.value` can resolve. Do **not** also push
each shape field as a top-level `ReactiveBindingFact` — that invents edges for
bare `field.value` when the consumer never destructured. Destructured calls
(`const { field } = useX()`) remain the only object-bag path that seeds a local
`field`. Scalar **`Factory`** exports are different: `const x = useFlag()` seeds
`x` as a top-level binding of the factory kind (no instance bag).

The graph retains `composable_instances` (v5) so template joins can resolve pure
member chains `bag.field` / `bag.field.value` (and static optional forms
`bag?.field` / `bag?.field?.value`) after module re-trace. Free-id extraction
only yields `bag` for those expressions; without the instance map, template
would stay quiet. Operator-bearing expressions (`bag.field + x`) and computed
brackets (`bag?.[k]`) stay quiet — under-approx, not a mini expression
evaluator.

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

## Text report color is CLI-injected

ANSI styles apply only when `ReportContext.color` is true (CLI `--color`).
`auto` enables color for TTY stdout unless `NO_COLOR` is set (non-empty);
`FORCE_COLOR` / `CLICOLOR_FORCE` (non-empty) force on. Pipelines and reporter
snapshots keep color off so byte-stable fixtures stay green. JSON / SARIF /
GitHub never paint.
