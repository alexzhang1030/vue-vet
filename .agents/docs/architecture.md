# Architecture

## Monorepo analysis pipeline (end-to-end)

One open pipeline across crates — no side-pocket "magic" hosts. Each crate is a
stage owner; `lib.rs` files stay thin façades where the crate has been split.

```text
vue-vet CLI / --lsp / --mcp
  -> vue_vet_session
       discovery     WorkspaceInputSnapshot + PackageIndex
       parse         vue_vet_vize (SFC) / vue_vet_oxc (JS/TS) → File Fact IR
       project       vue_vet_project pipeline
                       context → structural → passes(enrichment)
                       → reactivity Trace → layers → model-demand join → project rules
       reactivity    vue_vet_reactivity (trace / summary / link)
       rules         vue_vet_rule_query → vue_vet_rules + vue_vet_practice
       finalize      DiagnosticFinalizer → vue_vet_core ScanSummary
  -> vue_vet_reporters | vue_vet_lsp | vue_vet_mcp
```

Crate ownership (read before editing that stage; each crate's `README.md`
carries its module layout and public API — see [docs/crates.md](../../docs/crates.md)):

| Stage | Crate | Notes |
| --- | --- | --- |
| Stable contracts | `vue_vet_core` | facts / diagnostics / `Rule` — no Oxc/Vize types; Oxc types stay in `vue_vet_reactivity::trace`. |
| Adapters | `vue_vet_vize`, `vue_vet_oxc` | short-lived AST → facts only; SFC parse is `vize_croquis::sfc`, never `vize_atelier_sfc`. Template allocation/memo/condition/ref relations are Vue Vet-owned DTOs recorded in the Vize walk (start-tag spans cannot prove descendants) and joined with Oxc demand facts. |
| Project graph | `vue_vet_project` | see [`vue_vet_project` pipeline](#vue_vet_project-pipeline-crate-layout) |
| Cross-file seeds | `vue_vet_reactivity` | `ModuleSource` + `trace_modules`; Oxc-taking APIs under `::oxc`; `ModuleSummary` boundary; under-approx |
| File rules | `vue_vet_rules`, `vue_vet_practice` | consume facts via `vue_vet_rule_query`; practice off score |
| Orchestration | `vue_vet_session` | thin façade; `pipeline` stages discovery → facts → project → rules → finalize |
| Surfaces | `vue_vet_cli`, `vue_vet_lsp`, `vue_vet_mcp`, `vue_vet_reporters` | thin |

## Current vertical slice

```text
vue-vet CLI
  -> vue_vet_session (config, cache, scan, explain, workspace paths)
       -> immutable WorkspaceInputSnapshot (one walk/read; SourceStore Arc<str>)
       -> PackageIndex + normalized FileId identities
       -> parallel per-file facts (Vize SFC / one Oxc module parse)
       -> vue_vet_project edges + vue_vet_reactivity module seed linking
          (per-file structural cache + bounded incremental module state)
       -> apply module graphs onto setup and dual ordinary (#script) blocks
       -> parallel seed-aware vue_vet_rules + vue_vet_practice
       -> one DiagnosticFinalizer (severity, suppressions, dedup, sort)
       -> vue_vet_core diagnostics, spans, scoring (sorted for determinism;
          category `practice` excluded from score / default CI exit)
  -> vue_vet_reporters text or JSON rendering
  -> CLI output and CI exit policy
```

### Performance model (oxlint-inspired)

- **One retained input snapshot per session revision** — discovery walks and
  reads each source, manifest, and resolver input once. Cache lookup and a
  cache-miss analysis share that snapshot; `apply_changes` updates only named
  paths. Sources are retained as `Arc<str>`; package environments are parsed
  once into `PackageIndex`.
- **Files parallel, pipeline per file sequential** — parse / facts / seed-aware
  rules use Rayon (`--threads N`). The same bound is passed to module tracing,
  so `--threads` constrains the complete scan.
- **Rules are pass-based, not "each rule re-scans everything"** — `Rule`
  exposes oxlint-style hooks over Vue Vet facts (not dependency AST):
  `run_once` for whole-file aggregation, `run_on` + `fact_kinds` for a per-fact
  visitor with a bitset interest set. `RuleRegistry` runs `run_once` once per
  rule, then a **single walk** over each fact surface dispatching only bucketed
  interested rules. Rules report immediately; they do not `collect` and
  re-scan.
- **Two-phase bounded module reactivity** — `TraceModulesOptions::max_workers`
  caps both Rayon phases; never one native thread per module. The Oxc adapter
  extracts script facts, the local graph, and opaque module summaries from one
  semantic; the coordinator resolves seeds from those summaries. Modules whose
  seed plan cannot materialize reuse their local graph; a consumer reparses
  only when its source or resolved seed plan changed, and unchanged final
  graphs are reused from `ModuleTraceState`. Oxc arena values never cross a
  thread or adapter boundary; the workspace forbids `unsafe_code`.
- **Dirty-set scheduling (parse locality)** — `apply_changes` retains the dirty
  `FileId` set in `PendingChanges`. `analyze_affected` returns the last
  snapshot when the revision is unchanged; otherwise `ChangeImpact` +
  `DirtyPlan` decide which files re-parse, which need environment/rule
  refresh, and which diagnostics to finalize. `DirtyPlan.rule_files` covers
  Vue SFCs **and** JS/TS/JSX/TSX. Cached file `RuleEnvironment` is reused when
  `force_full_parse` is false and the file is outside `impact.environment`.
- **Incremental project stages** — `ProjectSession` retains the source
  snapshot, per-file facts, raw file diagnostics, structural edge partitions,
  module seed plans/final graphs, and the reverse dependency index. After a
  warm persist scan the tracer receives a source-dirty subset of
  `Arc<ModuleSource>` plus `retain_cached_modules`; linking compares live
  surfaces in place and merges cached summaries only on a miss. Layers read
  `ModuleTraceState` and store `Arc<[Arc<ModuleReactivity>]>`;
  `LayeredInputKey.modules` is sorted by `ModuleId` so a leaf rebuild probes
  with binary search. Default CLI reports use count-only reactivity stats;
  `--print-reactivity` attaches `modules_detail`.
- **Atomic session publication** — the workspace revision, retained input
  snapshot, and committed analysis state share one `SessionCore` lock.
  Analysis captures `Arc` snapshots under the lock, computes outside it, and
  commits only when the same revision is still current.
- **Resolver-context parity** — `ProjectContext.epochs` tracks independent
  counters for package / lockfile / tsconfig / Nuxt / source-membership.
  Context changes are not re-parse: tsconfig/lockfile/membership bump
  resolution and indexes; package Vue-version / Nuxt declarations refresh
  environments, rules, and component conventions. Incremental results remain
  equal to a clean scan.
- **Shared Rayon pool** — session builds one persistent pool lazily on the
  first real scan (never on warm cache hits) and passes
  `TraceModulesOptions { reuse_current_pool: true }`; standalone
  `trace_modules_with_options` installs a dedicated pool sized to
  `max_workers`.
- **Analysis state preparation** — each run seeds a candidate from the previous
  committed state, shares `ProjectGraphState` and file/diagnostic maps via
  `Arc` (copy-on-write / `share_from` on cache hit), and reuses
  `Arc<AnalyzedCandidate>`. Graphs are shared by `Arc` and mutated with
  `Arc::make_mut`; export resolution is a worklist; session input updates fork
  the snapshot once.
- **Partial module outcomes** — parse/link failures are scoped
  `AnalysisIssue`s; one bad module never forces every other module back to an
  isolated local graph.
- **Determinism after concurrency** — diagnostics are sorted in
  `ScanSummary::finish`; module results are sorted by module id.
- **Still single-process Rust** — no JS rule host; the pass walks Vue Vet
  facts, not Oxc/Vize nodes.

### Locality plan shape

Dirty-set scheduling and shared IR landed through
[#107](https://github.com/alexzhang1030/vue-vet/pull/107) /
[#108](https://github.com/alexzhang1030/vue-vet/issues/108). Do **not**
pursue a generalized unified AST IR; further locality work belongs in narrower
dirty linking / diagnostics plans.

```text
ChangeImpact { parse, environment, resolution, component_index, membership }
  → DirtyPlan { parse_files, structural_files, module_summaries,
                export_closure, rule_files, diagnostic_files }
  → stage work counters (files_parsed, cached_modules_merged,
                         seed_plans_recomputed, export_resolve_ran,
                         seeded_reparses, COW clones, …)
```

Standing decisions: `AnalysisProduct` lets LSP publish diagnostics without the
full graph DTO (`analyze_affected_product` / `diagnostics_for`). Linking cache
skips export/provide/seed fixed points when the linking surface (imports,
exports, locals, provides, injects — not `local_graph`) and links are
unchanged. Disk-cache hits stay cache-load cheap; empty session IR is seeded
on the first dirty analyze via `force_full_parse`. Single-file algorithms are
`TrackingScopeIR`, Vize bottom-up `SubtreeSummary`, `SfcBlockRevisions`,
`returns_by_function`, and a shared `SourceContext` (`Arc<str>` +
`Arc<LineIndex>`). Script reuse is `can_reuse_script = reuse_template && …`
because template-ref demand facts are joined during the Oxc script walk.

### Semantic IR layers

Vue Vet keeps small domain IRs rather than a unified AST:

```text
Parser IR (Vize AST / Oxc Semantic)     — short-lived, never cached across adapters
        ↓
File Fact IR (SfcFacts / ScriptFacts / TemplateFacts)  — stable, rule-facing
        ↓
Module Semantic IR (ModuleSummary)     — cross-file seeds; lifecycle-scoped
        ↓
Project Relation IR (ProjectGraph / ReactivityGraph / PropFlow)
        ↓
Diagnostics IR (Diagnostic / EditPlan)
```

File Fact IR sub-surfaces owned by the Oxc adapter (each is a serializable
field on `ScriptBlockFacts`; the Oxc AST never leaves `vue_vet_oxc`):

- `source_contracts` (`vue_vet_oxc::source_contracts`) — proven Vue API
  source-identity sites. Eligibility and dispatch share one `contract_sink`
  table (`ContractSink::{WatchEffectFamily, ToRef, EffectScope, CustomRef,
  Computed, …}`) with generic source5. Demand-gated contracts use a
  function-level execution region plus source-order barriers; VueUse demands
  (`vueuse.rs`) require exact `@vueuse/core` / `@vueuse/shared` provenance;
  collection capability is a dedicated poisoned-root query, separate from
  source5 `uncertain` / `escaped`; Proxy allocation proof lives in
  `clone_boundary.rs`. Derivation-practice (`sync_ref_one_way`,
  `conditional_watch_source`) and scheduling-practice
  (`scheduling_practice`) facts sit beside them. The traps are in
  [gotchas](./gotchas.md#source-contract-lanes-oxc-source_contracts).
- `lifetime` (`ReactivityLifetimeFacts`) — watcher / effect-scope cleanup
  contracts for the `lifetime` rule group. Provenance is **named Vue imports
  and aliases only** (namespace `Vue.watchEffect` stays quiet). Same-invocation
  `await` and deferred native Promise / scheduler / `nextTick` boundaries are
  in scope; unknown owner arguments and unproven scope identity abstain. Scope
  ownership is per invocation; nested-watch and detached-scope facts share the
  same lifetime index. Runtime evidence: `just oracle-lifetime`,
  `just oracle-stale-settlement`, `just oracle-cleanup-identity`; per-rule
  semantics live in `docs/rules/reactivity/*.md`.
- `template_ref_demands` — pre-flush and `v-memo`-blocked template-ref demands
  joined with Vize allocation relations during the script walk.
- `runtime_export_spans` — runtime ES-module exports for the
  `vapor-migration` group (named exports only when not type-only, plus every
  default and star export).
- `TemplateElementFact::has_key` includes proven object-form `v-bind` keys;
  `is_component` is Vize `ElementType` / JSX identifier-reference adapted into
  stable facts; Vize owns directive extraction.

**Vue JSX/TSX** is an Oxc-owned third surface (not Vize): JSX lowers into the
same `TemplateFacts` so template rules and `ComponentUsage` edges reuse without
a parallel pattern engine or Babel transform. Structural JSX facts are collected
only when the script language is `jsx`/`tsx`; `TrackingScopeKind::Render` and
JSX expression joins apply inside recognized render bodies (options/`setup` →
render, exported functional components, same-file `defineComponent` alias and
one-hop forwarders). Session runs the Vue file-rule registry on `.jsx`/`.tsx`
always, and on plain `.js`/`.ts` only when local or seeded facts warrant it.
See [#134](https://github.com/alexzhang1030/vue-vet/issues/134).

The opt-in `vapor-migration` group (`category: migration`) is an off-score
assessment channel like practice. `vue_vet_project` emits its Info IDs from
facts + the project graph; session default-offs them unless
`assessment = "vapor"`, `--group vapor-migration`, or a `[rules]` override.
`Diagnostic.assessment` carries `convertible` and `aggregate`
(`ready` only when complete, no open/blocking checks, and `runtime-envelope`
is `compiler-candidate`). Evidence:
[vapor migration research](../../docs/research/vapor-migration.md).

`ModuleSummary` is the formal cross-module boundary: imports, exports,
provides/injects, local reactivity, and no Oxc/Vize nodes. Session file-rule
reuse is keyed by `FileRuleInputKey` (source and `RuleEnvironment` digests,
final module graphs by `Arc` content equality). Shared block access and
control-flow queries over facts live in `vue_vet_rule_query`.

Every built-in lint rule is a self-contained module under
`vue_vet_rules/src/rules`; the parent module only assembles the registry (no
central match). Tracking-graph / after-await packs may live as a **matrix
family** under `rules/matrix` (shared detection type + unique `RuleMeta`
catalog). Practice suggestions live in `vue_vet_practice`: recipe metadata plus
thin `Rule` implementations over the same facts, `category: "practice"`, an
optional `recommendation` payload, off the score / default CI exit; some keep a
historical id segment (`vue-vet/reactivity/prefer-use-template-ref`) for
configuration stability. The session derives per-file Vue capabilities from
`PackageIndex` and passes them in `RuleEnvironment`.

The Oxc adapter delegates reactivity construction to `vue_vet_reactivity`,
which records Vue-resolved bindings and **tracking scopes** with classified
demand reads and guard evidence; `effects` is a projection of effect-family
scopes for existing consumers. Template joins include interpolations, directive
expressions, template `:style` (`surface = "style"`), and `<style>`
`v-bind(ident)` (`surface = "style-v-bind"`). See
[reactivity tracer](./reactivity-tracer.md). Configuration changes rule
enablement and severity after semantic analysis; suppressions are applied after
diagnostic normalization and emit findings when unused.

## Stable boundary

Vue Vet's normalized facts and diagnostics are the architectural seam.
Dependency AST objects must not cross into public rule, reporter, cache, LSP,
or agent contracts.

Default `vue_vet_reactivity` consumers use `ModuleSource` plus `trace_modules`
/ `trace_modules_with_options` / `prepare_standalone_module_source`, then
`explain_tracking_scope`; those take Vue Vet types only. Every function that
takes Oxc `Semantic`, AST, `Span`, or `NodeId` lives under
`vue_vet_reactivity::oxc`, and `vue_vet_oxc` imports from that namespace. Oxc
`SemanticBuilder` must `.with_build_nodes(true)` wherever facts walk
`semantic.nodes()`.

`ReactiveBindingFact.alias_of` records `const alias = known` on the same
binding record. Rules compare alias-aware targets with
`vue_vet_rule_query::same_reactive_target` using
`FactRef::TrackingScope.block_kind` and `script_block` so ordinary script and
setup same-name bindings stay distinct. General alias analysis is out of scope.

## `vue_vet_reactivity` crate layout

The tracer crate is a **library of collectors**. `lib.rs` stays a façade;
stages live in `trace/`:

```text
trace/mod.rs        single-file entry + orchestration
trace/kinds.rs      vue callee / binding kind / import / span helpers
trace/bindings.rs   reactive binding collectors (typed / props / aliases / route)
trace/local.rs      same-file composable usage
trace/context.rs    scope_context + HOF / toValue / deferred + `ScopeNodeIndex`
trace/reads.rs      scope reads + classify + guards + `ScopeIrIndex`
trace/writes.rs     scope writes + assignment_only + identifier getters
trace/uncertain.rs  uncertain accesses + watch sources
trace/scopes.rs     tracking / render scope assembly
trace/inject.rs     provide/inject sites + unique-key resolve
trace/follow.rs     same-file zero-arg helper walk + file `LocalCalleeIndex`
trace/expr.rs       paren / TS peel shared by assignment-only and factories
trace/plugin.rs     NamedApiBag / TracerPlugin / TraceConfig
trace/branch_hygiene.rs  pure A4 all-path coverage
trace/render.rs     structure-first render bodies
trace/notification/ lost-notification source views / bypasses
trace/summary/      prepare / return shapes / export lattice / link (cross-module seeds)
src/tests/          domain modules + shared helpers (not one file)
```

Do not grow `trace/mod.rs` or `src/tests/` with another collector family or
fixture corpus — add a sibling module. Layouts for the adapter and surface
crates are in their `README.md` files; the same rule applies there: `lib.rs`
/ `main.rs` stay façades.

## `vue_vet_project` pipeline (crate layout)

The project crate is an **explicit stage pipeline**. `lib.rs` is a thin
façade; orchestration lives in `pipeline.rs`:

```text
context          ConventionsLoad → ProjectContext
structural       StructuralLink (import/component edges)
passes           enrichment (see below)
pipeline         Trace handoff + ProjectGraph assembly
layers           template joins + prop-flow
model_demand     defineModel default / parent demand join
rules            unresolved-import / unused-component
vapor_migration  opt-in assessment IDs
model / state    DTOs + retained incremental partitions
resolve / conventions   oxc_resolver + Nuxt maps
```

### Analysis enrichment passes (not user plugins)

Nuxt / package-shape specialization lives in **compile-time Rust enrichment
passes** over Vue Vet IR — not AST Traverse (Oxc/SWC), and not a dynamic JS
plugin host. Diagnostic [`Rule`](../../crates/vue_vet_core/src/lib.rs) passes
consume the enriched facts; enrichment passes must not `report` diagnostics.
Each step is a named `struct` with an inherent `::run(...)` (see
`ENRICHMENT_STEPS` in `vue_vet_project::passes`); there is no empty metadata
trait and no dynamic plugin ABI.

```text
ConventionsLoad           (context + conventions → ProjectContext maps)
  -> StructuralLink       (structural.rs ordinary edges;
                           NuxtImportsSeedPass::run for bare auto-imports)
  -> ExternalSummaryLoad  (ExternalSummaryLoadPass::run)
       └─ SummaryMerge    (ProvisionalFactoryMergePass::run at each loaded
                           module — same traversal, not a hidden side effect)
```

After enrichment: SeedPlan / Trace (via `vue_vet_reactivity` from `pipeline`)
and RuleRegistry (file rules outside this crate). Constraints: IR only
(`ProjectContext`, `ModuleLink`, `ModuleSummary`, `ExportState`); sorted
outputs; quiet under-approx; no `dlopen` / npm analysis plugins before a
separate ADR.

### Reactivity tracer plugins (`vue_vet_plugins`)

Ecosystem **named API bag** contracts (Nuxt `useAsyncData` / `useFetch`,
vue-i18n `useI18n`, …) are **not** hardcoded inside `vue_vet_reactivity`. The
engine accepts a `NamedApiBag` catalog via `TraceConfig` /
`TraceModulesOptions`; the published `vue_vet_plugins` crate implements
`TracerPlugin` and exposes `default_named_api_bags()` /
`default_trace_config()` / `ensure_default_plugins()`. Oxc single-file
analysis, `vue_vet_project` graph builds, and `vue_vet_session` install the
default catalog at the boundary; pure `vue_vet_reactivity` callers stay empty
until they pass a catalog. Compile-time Rust only — no `dlopen` / npm plugin
ABI. See [the crate README](../../crates/vue_vet_plugins/README.md).

## Crate evolution

Every workspace crate ships a `README.md` under `crates/<name>/`; the index is
[docs/crates.md](../../docs/crates.md). New rule capabilities extend these
semantic and product boundaries only when a working vertical slice exercises
them; there is no separate pattern-engine boundary.

`vue_vet_rule_query` is the workspace-internal fact-query layer used by
`vue_vet_rules` and `vue_vet_practice`: depends only on `vue_vet_core`,
exposes no Vize or Oxc types, is not published, and returns borrowed views.
Put a helper there when two or more rules repeat the same block walk or
control-flow predicate.

`vue_vet_session` owns the long-lived project analysis handle: config load,
cached/fresh scans, unsaved overlays, per-file fact state, reverse
dependencies, rule/finding explain, workspace path containment, and the
**product rule-group table**. Canonical groups (`tracking`,
`source-contracts`, `lifetime`, `derivation`, `project`) map composed registry
IDs one-to-one; `vue_vet_core` holds only serializable group DTOs. `--group`
is applied to the effective `vue-vet.toml` **before** analysis by setting
non-selected known IDs to `off`, so cache identity, score, exit, edits, and
explain share one config. `--list-rules` prints the composed registry
independent of project configuration. Overlay analysis bypasses the
content-addressed cache. A file or module failure becomes a scoped
`AnalysisIssue`; fatal root or configuration errors still fail the request.

### Published library crates

`vue_vet_core`, `vue_vet_reactivity`, and `vue_vet_plugins` are published to
crates.io (in that order); the rest of the workspace stays `publish = false`.
Published packages omit in-tree fixtures and the runtime oracle. Path
dependencies between publishable crates carry an explicit `version`. Tagged
releases publish them from `.github/workflows/release.yml` after quality
gates. End-user CLI installs go through npm (`@vue-vet/cli` + platform
packages) or GitHub Release archives; version numbers stay aligned across the
Cargo workspace, npm, and `v*` tags. Details: [install docs](../../docs/install.md).

## Reporting and edit planning

`vue_vet_reporters` consumes Vue Vet-owned `ScanSummary` values plus an
explicit `ReportContext` (scan mode, framework, analyzed files, completeness,
skipped-check reasons, color) and owns deterministic text, versioned JSON,
SARIF, and GitHub rendering; the CLI retains stdout, operational-error
messages, and exit policy. Renderers return content without a terminal
newline. Text snapshots are byte-for-byte compatibility gates (color off);
JSON snapshots are versioned wire-contract gates. JSON v1 is the shared fact
layer for CI and agent surfaces: consumers must use `complete` and exact
analyzed-file coverage rather than treating an empty findings array as a clean
scan. Contracts: [JSON output](../../docs/json-output.md),
[SARIF / GitHub](../../docs/sarif-github.md).

The shared edit contract lives in `vue_vet_core`: a text edit carries a
repository path, checked byte range, replacement, safe/unsafe applicability,
and originating rule ID; `EditPlan` normalizes ordering and rejects overflow,
overlap, and order-dependent insertions. The CLI's fix module previews or
applies active safe edits from the original source in reverse-range order with
atomic single-file replacement, bypasses cached results, and fails closed on
multi-file plans (issue #9). Producers and modes:
[edit model](../../docs/edit-model.md).

## Identity and determinism

Rule IDs and diagnostic fingerprints must remain stable enough for baselines,
diff mode, SARIF, LSP, and agent consumers. Results are sorted independently of
traversal or hash-map order. Discovery converts physical paths exactly once to
workspace-relative normalized `FileId`; diagnostics, edits, graphs, caches,
baselines, LSP, and reporters compare that identity exactly. Suffix matching is
forbidden. Coverage reports analyzed source files separately from manifests,
lockfiles, and resolver inputs that invalidate the graph.

## Thin editor host, LSP, and MCP

`editors/vscode` is a **thin** VS Code host for reactivity visualization. It
spawns the Rust CLI (`--format json --print-reactivity`), maps `*_details`
byte spans onto decorations / hover / a TreeView, prefers
`modules_detail[].binding_nav`, and shells `--explain-scope file:@offset` for
Explain Scope. It does not start an LSP client and must not grow a parallel
tracer.

`vue-vet --lsp` (`vue_vet_lsp`) publishes `textDocument/publishDiagnostics`
from open-buffer overlays with the same opaque finding ids as JSON
`diagnostics[].id` (in LSP `data`) plus the document version. A debounced
latest-wins gate admits one blocking analysis; stale work cancels between
pipeline phases and its commit is rejected under the session lock. Safe
quick-fix code actions map active safe edits to versioned `WorkspaceEdit`s
(client applies; server never writes). Hover converts the UTF-16 caret to a
byte offset and asks `session.explain_scope` — the same `ScopeExplain` as CLI
`--explain-scope`. Diagnostics publish may use
`AnalysisProduct::DiagnosticsOnly`; the committed snapshot keeps the full
graph so hover does not re-trace.

`vue-vet --mcp` (`vue_vet_mcp`) exposes newline-delimited JSON-RPC 2.0 tools
over stdio (no `Content-Length` framing) for scan, explain, explain-scope, and
safe-fix preview with the same workspace path bounds; it never applies edits
and keeps one session per resolved tool path. Request-level cancellation
remains issue #12 work. Details: the [LSP](../../crates/vue_vet_lsp/README.md)
and [MCP](../../crates/vue_vet_mcp/README.md) READMEs.

## Project intelligence

Cross-file findings derive from a Vue Vet-owned graph of imports, components,
composables, routes, stores, and Nuxt conventions. Diff mode must invalidate
and re-run affected graph consumers; it cannot scan only changed files and
silently lose a newly caused project-level failure.

`vue_vet_project` consumes serializable `SfcFacts`, uses repository-relative
file IDs, stores source evidence on every edge, and publishes its exact file
inputs for cache invalidation; `CONVENTIONS_VERSION` changes whenever Nuxt
directory or naming behavior changes. Nuxt ownership is config-file based
(`package.json` / `nuxt.config.*`), with exported-config `modules`, literal
`srcDir`, and cycle-safe static `extends` layers resolved through the existing
resolver — configs are never executed, and layer config bytes belong to the
retained snapshot. The project graph supplies resolved module edges
(standalone JS/TS **and** preferred SFC script blocks) to `vue_vet_reactivity`;
extracted `.vue` scripts use Vize block offsets plus the original SFC as
`span_source`, and template joins are re-applied after cross-file seed
linking. Details: [project graph](../../docs/project-graph.md).

The cache stores `ScanSummary` and `ProjectGraph` under a content key that
includes every source body plus configuration, tool, dependency, convention,
and ruleset versions (`crates/vue_vet_cache/src/lib.rs`). Baseline and diff
filtering happen after cache lookup so presentation choices do not fragment
semantic cache entries; fix modes force a fresh scan before planning. Details:
[cache, baseline, diff](../../docs/cache-baseline-diff.md).

See [technology stack](./technology-stack.md), [conventions](./conventions.md),
and [the roadmap](../../ROADMAP.md).
