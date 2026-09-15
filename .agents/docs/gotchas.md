# Known gotchas

Traps already paid for, grouped by area. Each entry states the trap and the
rule that avoids it; the code or test named is the evidence. History lives in
`git log`.

## Build, CI, release

### Keep Cargo targets local to one worktree

Give each worktree its own `CARGO_TARGET_DIR`: a shared target can keep an
older local-crate artifact `Fresh` while a downstream crate compiles against
a newer fact schema. Freeze comparison binaries outside the target directory
with commit, build command, and hash.

### Native-size budget

`fixtures/quality/native-size-budget.json` is a 3 % regression guard on the
stripped matrix binary, not a product ceiling: rule lanes add code and a
stacked series crosses the line every few lanes. Re-pin the candidate to the
failing PR's own `pkg.pr.new` matrix run; never shrink fixtures, loosen the
margin, or re-pin for profile / dependency changes without the CLI and bench
gates in [quality baselines](../../docs/quality-baselines.md). Each platform
gate uses its own artifact; Linux ARM64 moves in 64 KiB ELF page steps.

Production work counters must stay zero-sized: `WorkCounter` fields in the
Oxc source-contract, lifetime, template-demand, and tracer collectors are
`Cell`s only under `cfg(test)`; `LAST_WORK` / `last_*_work` are test-only.
Leaving them on the production path once failed every native-size gate.

### Benchmarks

- Cargo forces benchmark targets to `panic = "unwind"` even when the profile
  says abort, so a Divan run measures the unwind path. CodSpeed uses
  `profile.codspeed` (`panic = "unwind"`, `lto = false`); the shipped CLI is
  `cargo build --release` (abort) and is timed separately.
- `codspeed-divan-compat` 5.0.1 has no `threads` attribute under
  `cfg(codspeed)`; use `SessionOptions { threads: Some(1) }`. `cargo-codspeed`
  5.0.1 clears a package's staged suite directory on every build, so pass all
  of a package's `--bench` targets in one invocation (see
  `just bench-codspeed-build`).
- Keep filesystem teardown out of measured closures —
  `scan_diff_filter_nuxt_graph` bounced ±15 % when `remove_dir_all` sat next
  to an ~8 µs retain. Validate with `just bench-codspeed-build` + `just bench`.

### Release publishing

crates.io returns **403** to clients without a descriptive `User-Agent`
(bare `curl` included); index polls in `release.yml` send one, and re-runs
skip already-published versions. `npm publish` can lag the registry — wait
with `npm/scripts/wait-registry.mjs`. Only `@vue-vet/cli` declares
`bin.vue-vet`; platform packages that also declared it left
`node_modules/.bin` empty under npm 10 (install smoke uses `npx`).

## Spans and encoding

### Offsets are SFC-absolute bytes

Vize block locations are byte offsets into the original SFC; the template
parser and Oxc report offsets relative to the extracted block. Add the block
`loc.start` exactly once. Ordinary script and script setup stay separate fact
blocks; never run one pattern language over the whole `.vue` file. Persist
ASCII, Unicode, multiline, and CRLF snapshots when span math changes; CRLF
fixtures need `.gitattributes` `-text`.

### Element spans are start-tag only

`TemplateElementFact.span` covers the opening tag, not the subtree. Nesting
(`v-if` / `v-for` / slot / `Suspense` ancestry, `has_labelable_descendant`,
template-ref relations) is recorded as explicit flags or
`TemplateAllocationFact`s during the Vize walk; never test span containment
to recover ownership. Implicit default-slot content sets `has_slot_ancestor`.
Kebab-case component tags trust the project `ComponentUsage` edge, not Vize
`is_component`. Template expressions are parsed with Oxc
(`template_simple_identifier`, `template_memo_tuple`), never string-matched.

### Paths are identities, not suffixes

Discovery is the only place a physical path becomes a normalized
workspace-relative `FileId`; later stages compare exactly (`ends_with` picks
the wrong `App.vue` in a monorepo). LSP positions are UTF-16 via
`vue_vet_core::LineIndex`; document identity goes through
`ProjectSession::file_id_for_path`. `ProjectGraph` node ids are
`file:{path}` while module graphs use bare paths — strip the prefix before
module lookups. `node_modules/pixi.js` is a directory whose extension is
`js`: walks skip `node_modules` and resolve symlinks / unknown types through
`Path::is_file()`.

## Vize and Oxc adapters

### Pins

Vize is exact-pinned and moves fast; upgrading is a compatibility task
([procedure](../../docs/vize-compatibility.md)). Vize 0.387 requires Oxc
0.142; keep Oxc on the Vize family (never two Oxc graphs). `oxc_resolver`
stays `11.21.0` until a dedicated upgrade reviews resolve-quiet behavior
(see the comment in `Cargo.toml`). Template `SourceLocation` is a byte `Span`
only; tag / name fields are `&str`. Oxc 0.142 `SemanticBuilder` leaves
`Semantic::nodes` empty unless `.with_build_nodes(true)` is set — node-walk
collectors then succeed with empty results and rules go quiet.

### Configuration is part of diagnostic identity

Preset expansion runs before explicit overrides; severity overrides precede
exit-policy scoring; suppressions run after diagnostics exist so unused
directives are reported. Cache keys include the effective configuration.

### Accessible content is not `has_children`

`has_children` is structural (`valid-v-html` / `valid-v-text`). Name rules
(`anchor-has-content`, `button-has-content`, `heading-has-content`) use
`has_accessible_content`: text, interpolation, `v-text` / `v-html`, an `img`
/ `area` with non-empty `alt`, or a custom component child. A static `title`
is not a name — it only feeds a safe `aria-label` insert. Tooltip / menu
wrappers with a name-like prop mark descendants `has_accessible_name_ancestor`.

### Safe fixes need complete source coverage

A diagnostic span is not a replacement span: the `autofocus` fact covers the
name only, so just the boolean form is removed. Producers reconstruct the
quoted extent from source (`aria-hidden="true"`, `:arg.sync`,
`@event.native`) and stay report-only otherwise. Fix ranges are
original-source byte ranges applied from the end; the executor is
single-file and fails closed on multi-file plans until issue #9. See
[edit model](../../docs/edit-model.md).

### JSX and SFC macros

Vue JSX/TSX is Oxc-owned: JSX lowers into `TemplateFacts`; never route it
through Vize. Plain `.js`/`.ts` join the file-rule registry only when local or
seeded facts exist (empty TS in `pending_vue` regressed CodSpeed `scan_*`).
Skip JSX collection unless the language is `jsx`/`tsx`; skip `defineComponent`
forwarder walks without a Vue factory import. `defineModel` / `defineProps`
are `<script setup>` macros: rules that recommend them gate on
`ScriptKind::Setup`. `no-deprecated-filter` requires a filter-shaped RHS and
skips `.jsx`/`.tsx` (TypeScript unions are the false positive).

### `<script vapor>` is setup

Vue flips `<script vapor>` to setup; Vize keeps it on `descriptor.script` with
`vapor` in attrs, so treat `ScriptBlockFacts.vapor` as setup. Dual-script
fixtures are `<script>` + `<script setup>`. Runtime-export blocking reads
`ScriptBlockFacts.runtime_export_spans` — never byte-scan for `export`.

## Reactivity tracer

Product stance and the graph contract live in
[reactivity tracer](./reactivity-tracer.md); these are the traps.

### Tracking semantics

- Scopes subscribe only to reads reached synchronously. Reads after a
  top-level `await` are `AfterAwait`; `then` / `nextTick` / timers are
  `OutsideTracking`. Keep every direct read — consumers need earlier
  unconditional reads to suppress false positives.
- `watch(ref)` records `property: Some("value")`; bare `watch(reactiveObj)`
  records the deep root `*`, never invented keys. Sync HOF callbacks
  (`filter`, `map`, `String#replace`, `Array.from(_, fn)`, `JSON.parse(_, fn)`)
  run inside the parent scope. `toValue(() => …)` tracks; `unref` does not
  call. Only `const scope = effectScope(); scope.run(cb)` is a tracking body.
- `just oracle` compares edges to Vue `onTrack` JSON; it does **not** prove
  run counts (`just oracle-self-trigger` does). Vue 3.5 coalesces a sync
  self-assign in `watch*Effect` into one run; computed self-write is impurity,
  not a loop. `pauseTracking` / `enableTracking` are not public `vue` exports.
- `ReactiveGuardRole` is metadata on one Conditional read; the per-guard rule
  ids were withdrawn ([removed ids](../../docs/rules/removed-ids.md)).

### Same-file helper follow (dual-path)

Inline and helper-backed forms must agree. `follow_local_callees` (depth ≤ 2,
skip async / generator / args / import / method) is the single callee
enumerator for reads, `uncertain_accesses`, writes, and `assignment_only`.
Followed reads inherit **caller** guards via call-site proxies. Pause / resume
inside a helper projects onto the call end (Vue `shouldTrack` is
process-global); never merge helper pause events by file offset. Identifier
getters (`computed(load)`) resolve through `local_getter_parts`. Local
collectors look up `FileTraceIndex` / `ScopeIrIndex` instead of re-walking
`semantic.nodes()`. Parens / TS wrappers peel once (`peel_parens`).

### Absence rules and soft evidence

Absence rules (`no-computed-without-dependency`, `no-effect-write-without-read`,
`no-empty-watch-sources`, `no-watch-callback-as-tracking-scope`) try hard
evidence first and consult `uncertain_accesses` only when reads stay empty,
reporting `(maybe: …)`. `unknown_calls` / `follow_truncated` / non-empty
uncertain make `analysis_complete` false. Sync HOF callback params are **not**
soft evidence; untyped composable formals are. `&&=` / `||=` / `??=` are not
writes. Explain module matching is `vue_vet_reactivity::module_id_matches`.

### Typed ref parameters

Formals / declarators annotated `Ref` / `ComputedRef` / … (and outermost
`expr as Ref<T>`) seed classification via `ts_type_reactive_kind`; function
types with Ref-like formals publish `TypedCallbackParamSlots`. Only an
optional `value?` type literal is a Ref duck. Function-local `ref()` stays
out of top-level `bindings`; `prefer-computed` must not read that as private.

### Cross-module summaries

- Never concatenate files. Exports, composable returns, imported calls, and
  reads agree on Oxc symbol identity (`reference_resolves_to_binding`); bare
  auto-import seeds match by name only when no local symbol exists.
- Seed spans use the module's `source_offset` + `span_source`; return-shape
  resolution uses the binding `script_offset` — hard-coding 0 drops SFC bags.
- `const bag = useX()` seeds `composable_instances` only; shape fields are not
  top-level bindings. Dual scripts re-trace as setup plus `{path}#script`.
- `return { list, ...spread }` merges only a proven reactive bag
  (`open_reactive_spread`).
- Imported factories record `ValueFactoryCall` and re-resolve at publish;
  never mark Vue primitives as `ValueFactoryCall` (breaks incremental seed
  reuse). `createSharedComposable` / `createGlobalState` are identity
  wrappers; residual `MethodForward` must not block the factory.
- provide/inject links through a project-wide provide index: seed only when
  exactly one provide has a known shape; imported keys match
  `(specifier, export)`, local `Symbol()` keys by definition span.
  `inject(key) as Ctx` peels the assertion. Generic context factories keep
  `GenericMethodInstantiate` in `working_locals` until publish.
- Shape helpers stay off the `const x = ref(0)` cold path
  (`returns_by_function` lazy, helpers `#[inline(never)]`).
- Edge `to` is a bare binding name; `to_id` (`{module}:{name}@{offset}`) is
  the identity.

### Nuxt / auto-import seeds and external follow

- Bare `ref` without an import resolves only when Oxc marks it unresolved and
  the name is on the `vue` / `#imports` allowlist; a local lookalike wins.
- Imports maps load first-wins (`.nuxt/imports.d.ts`,
  `.nuxt/types/imports.d.ts`, `auto-imports.d.ts`); specifiers resolve from
  the **declaring** dts. Vite maps use `typeof import('./src/…')['name']`.
  Single-file scans walk up to the nearest `package.json`
  (`discover_workspace_boundary`) or root maps never load.
- Companion `.js` bodies load only for provisional `.d.ts` halves, size-capped
  by `EXTERNAL_COMPANION_MAX_BYTES` (parsing every bundle stalled Nuxt apps).
  Never invent `Factory(Reactive)` from an interface alone.
- Bundler resolve lands on `exports["."].import`; `prefer_types_declaration`
  remaps to `types` or follow parses an empty JS barrel. Inlined `.d.ts`
  bodies strip **all** import lines. Interface `extends` follow needs a
  visited set and depth bound. Follow budget is global and per-package;
  canonicalize pnpm store paths in budget keys.

### Style `v-bind` and block reuse

`<style>` `v-bind(ident)` uses surface `style-v-bind`; template `:style`
keeps `style` (one shared name dropped template reads). `SfcBlockRevisions`
fingerprints template / script / setup only; style is re-extracted, so do not
add it. `ModuleSource` equality ignores `span_source`; block reuse keys on
digest **plus** absolute loc.

## Source-contract lanes (Oxc `source_contracts/`)

### Demand proof is not source5 eligibility

Demand-gated contracts own a function-level execution region plus
source-order barriers; generic source5 still uses immediate
`ExpressionStatement` parents. Do not reuse demand reach as source5 evidence.

- VueUse identity requires exact `@vueuse/core` / `@vueuse/shared`
  provenance, never a named bag or `#imports` spelling.
- Foreign events use exclusive `partition_point` queries over **unique**
  event indexes; `events_by_block` is sorted and deduped in `Indexes::build`
  (`cached_result_shared_source_producers_grow_subquadratically`).
- `toRefs(state)` and `until(source)` are generic source5 escapes; demand may
  discount only a proven first-argument borrow. Only same-block straight-line
  writes prove `until`'s current value.
- Cancelled-filter proof needs two same-wrapper `useDebounceFn` calls with no
  await between, then an await of the earlier promise. An await inside a
  `watchIgnorable` updater is the ignore-window signal (`straight_awaits_in`),
  not a stack-wide barrier; passing a watched ref to `watchIgnorable` is not
  demand proof.
- Object literals execute computed keys and pattern defaults. Memoized
  closed-key sets are borrowed per key — cloning per call is quadratic.
- `SourceContractStats` is a fixed nine-`u64` snapshot pinned by `stats.rs`.

### Identifier escapes

- A ref passed as a bare identifier is `uncertain` / `escaped`, so
  `watch(n.value)` then fails `payload_uncertain` — source-parent behavior,
  pinned by `source_parent_identifier_watch_use_drops_unwrapped_payload_proof`.
  Do not weaken fixtures to hide it. Ordinary `state.n` pattern writes stay
  off that set so `no-watch-alias-old-new` still fires.
- Vue checks `isRef(source)` first, so a written or deleted `__v_isRef`
  retargets `toRef`. `no-toref-ignored-key` proves the marker immutable via
  capability evidence; pattern assignment, constructor args, and call / tagged
  receivers are capability escapes.

### Proxy identity: private fields, Map keys, structured clone

- `reactive` / `readonly` proxies are outside a class's private brand: a
  prototype method reading `this.#field` on the proxy throws; `toRaw` keeps
  the raw instance, whose `#field` writes do not notify. TS `private` is a
  different contract.
- `new Map([[raw, v]]).get(proxy)` is `undefined`; Vue only normalizes keys
  stored in `reactive(new Map)`. Facts require a fresh unresolved `Map`
  constructor and a distinct proxy from `vue` / `@vue/*` (not `vue-demi`);
  `reactive` vs `shallowReactive` differ.
- `reactive()` returns the raw target for `markRaw`, `__v_skip`, frozen, or
  non-extensible input, so `no-proxy-structured-clone` uses allocation proof
  from `clone_boundary.rs`; native `structuredClone` identity is poisoned by
  unresolved `globalThis` writes. Demand after `return` / `throw` / `await` /
  `yield` is unreachable.

### Collection escape-depth exhaustion is Unknown

`poison_expr_bounded` walks helper / `new` / tagged / assignment arguments to
depth 8. Exhaustion on an identifier poisons that root; on any other leftover
the semantic-reference pass poisons every symbol inside it
(`capability_poisoned`) and taints canonical native constructor identity
(`intern_native_ctor`). Raising the depth alone would still trust a deeper
leftover.

### customRef lost-notification needs an executed, still-active consumer

`no-custom-ref-lost-notification` is a closed-local chain: post-flush first
runs are not subscribed at the call site; `{ once: true, immediate: true }`
stops before a later write; an executed `await` / `yield` ends the subscribed
prefix. Setters that do not store their first parameter with plain `=`,
generator / async accessors, coercing `==`, and replaced `_get` / `_set` are
Unknown and fail closed.

### Watch cleanup identity needs an executed schedule

`no-watch-cleanup-current-source` fires only after a proven acquisition and a
later distinct EventTarget **allocation** while the watcher is active.
Default `flush: 'pre'` coalesces sync writes, so two batched allocations are a
safe control. Const handle aliases canonicalize before stop / pause / resume;
an earlier conditional stop bounds the active interval. `null` handlers create
no listener. Runtime pin: `just oracle-cleanup-identity`.

### `once: true` is not a late cancellation-guard window

Vue 3.5 wraps a `once` callback as `_cb(...); watchHandle()`, so `stop()` runs
every cleanup before the first `await` settles. `no-late-cancellation-guard`
abstains on literal `once: true` and stays Unknown for non-literal options.
`watch*Effect` has no `once`. Runtime pin: `just oracle-stale-settlement`.

### Template-only SFC edits re-run script analysis

`analyze_sfc_facts_reusing` sets `can_reuse_script = reuse_template && …`
because template-ref demand facts are joined from Vize allocations during the
Oxc script walk; relaxing it leaves demand facts pointing at the old tree.

## Project graph and resolution

- Resolution is `oxc_resolver`; do **not** reinterpret failed resolves as
  external. A small allowlist is external **before** resolve (`node:`,
  stylesheets, `virtual:…`, `uno.css`, `*/auto-routes`, `#imports`); bare
  Node builtins and failed `#…` virtuals are quieted **after** resolve. Bump
  `CONVENTIONS_VERSION` when quiet rules change.
- Canonicalize the scan root before building the resolver (`vue-vet .` must
  not leave alias targets as `"."`); on Windows strip the `\\?\` prefix.
- Nuxt component names are not file stems (`HeroDemo.client.vue` →
  `HeroDemo` / `LazyHeroDemo`); prefer convention naming plus
  `.nuxt/components.d.ts`; never execute `nuxt.config`.
- `unused-component`: barrel imports resolve to an index — also emit
  `ComponentUsage` by imported name; skip `.story.` / `.test.` / `.spec.`.
- Cache corruption is a miss, not a failure. Diff filtering keeps every
  project diagnostic (a removed edge has no path back to the changed file).
  Static prop flow joins identifiers, `.value`, static member chains, and
  `v-model` → `modelValue`; whole-object `v-bind` and computed / call
  expressions stay quiet by design.

## Cache

Content keys hash `CACHE_FORMAT_VERSION`, `RULESET_VERSION`,
`REACTIVITY_GRAPH_VERSION`, `CONVENTIONS_VERSION`, and
`AnalysisStackIdentity::current()`; `just compat-matrix` gates those against
the pin and lock. Proving a field participates requires
`content_key_with_identity` and a different key — `assert_ne!` on a stale
string proves nothing. Keep
[cache-baseline-diff](../../docs/cache-baseline-diff.md) aligned with
`CACHE_FORMAT_VERSION`.

## Session, incremental locality, LSP, MCP

### Scheduling and identity

- Never one native thread per module: Oxc semantics are not `Send`.
  `TraceModulesOptions::max_workers` bounds both phases; only session sets
  `reuse_current_pool: true`. Never build the Rayon pool in
  `ProjectSession::open`.
- Cache lookup and cache-miss analysis share one `WorkspaceInputSnapshot`.
  Revision, retained input, and committed analysis share one `SessionCore`
  lock; CPU work runs on captured `Arc` snapshots and publishes only if the
  revision still matches.
- **Context invalidation ≠ re-parse** — `ProjectContext.epochs` bumps must
  not force `analyze_candidate()` on unchanged bytes. File-rule reuse compares
  `FileRuleInputKey` by `Arc` content equality; never `serde_digest` full
  graphs on the hot path.
- Never discard the dirty `FileId` set from `apply_changes`; schedule via
  `ChangeImpact` / `DirtyPlan`. **Dirty `FileId` ≠ dirty work** — prove
  locality with work counters (`files_parsed`, `module_summaries_visited`,
  `cached_modules_merged`, `seed_plans_recomputed`, `export_resolve_ran`,
  `seeded_reparses`) and `DirtyPlan.export_closure`.
### Sharing, not cloning

- `ModuleSummary` omits `Clone`; share `Arc<ModuleSummary>`, prefer
  `Arc::ptr_eq`. Linking reuse keys on imports / exports / locals / provides /
  injects + links — **not** on the full summary (which includes
  `local_graph`). `called_locals` is a phase-two skip index, never a key.
- Warm subset scans pass dirty `Arc<ModuleSource>`s
  (`trace_modules_incremental_from_arcs`) with `retain_cached_modules` +
  `drop_module_ids`; do not clone the live universe into `live_module_ids`.
  `trace_warm_leaf_edit_1k_modules` is the locality signal,
  `trace_1k_modules` is not.
- Layered template / prop graphs reuse `Arc<[Arc<ModuleReactivity>]>`; a leaf
  edit patches `LayeredInputKey` in place and must not `make_mut` the other
  N-1 graphs. `ProjectGraphState` holds `structural` / `module_trace` as
  independent Arc partitions; `AnalysisSnapshot` keeps its fields behind `Arc`
  so a no-op publish is refcount-only. Overlay updates fork once
  (`Arc::make_mut` + `apply_changes_in_place`).
- Never eagerly re-scan on a disk-cache hit to hydrate IR; publish `"hit"` and
  seed IR on the first dirty analyze via `force_full_parse`.
- Default CLI/JSON reactivity digest uses `ReactivityModuleStats::from_counts`;
  only `--print-reactivity` / TUI build labels.

### Surfaces

- Text color applies only when `ReportContext.color` is true (`--color`;
  `auto` = TTY and no `NO_COLOR`; `FORCE_COLOR` / `CLICOLOR_FORCE` force on).
  Snapshots and JSON / SARIF / GitHub never paint.
- `--progress` is **stderr only**: live TTY line, compact per-phase log when
  redirected, `auto` off under `CI`; never a percentage / ETA / filenames.
- MCP stdio is newline-delimited JSON-RPC, not LSP `Content-Length` framing
  (`mcp_stdio_round_trips_initialize_and_tools_list` spawns the real CLI).
  `vue-vet --mcp` keeps one `ProjectSession` per tool path; explain reuses
  `current_snapshot`.

## Vapor migration assessment

`Verdict::Ready` is the aggregate-only "recommended for direct conversion":
`complete`, no `blocked` / `unsupported` / `needs-verification`, and
`runtime-envelope` = `compiler-candidate`. Aggregate precedence: `blocked` >
`unsupported` > `needs-verification` > `compiler-candidate`.
`ENVELOPE_VUE_BUILT_IN_DIRECTIVES` / `ENVELOPE_SCRIPT_APIS` in
`vapor_migration.rs` are the admitted envelope from
[the research harness](../../research/vapor-migration/README.md); adding a
construct requires a passing runtime oracle fixture pair first.

## Product decisions worth re-reading

- **No parallel pattern engine.** Built-in analysis stays on Vize + Oxc facts
  ([technology stack](./technology-stack.md)).
- **The score is provisional.** `floor(100 × capacity / (capacity + raw))`,
  `capacity = files × 50`, weights Error 10 / Warning 3 / Info 1. Do not tune
  rules to move it.
- **Vue behavior is capability-gated.** Version-aware rules read the nearest
  `vue` requirement from `package.json`, include it in cache inputs, and stay
  quiet when unknown.
- **Diff mode is a graph problem.** Changed-line filtering tracks causality
  through the project graph rather than dropping findings outside the diff.
