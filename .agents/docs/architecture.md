# Architecture

## Monorepo analysis pipeline (end-to-end)

One open pipeline across crates — no side-pocket “magic” hosts. Each crate is a
stage owner; `lib.rs` files stay thin façades where the crate has been split.

```text
vue-vet CLI / --lsp / --mcp
  -> vue_vet_session
       discovery     WorkspaceInputSnapshot + PackageIndex
       parse         vue_vet_vize (SFC) / vue_vet_oxc (JS/TS) → File Fact IR
       project       vue_vet_project pipeline
                       context → structural → passes(enrichment)
                       → reactivity Trace → layers → project rules
       reactivity    vue_vet_reactivity (trace / summary / link)
       rules         vue_vet_rules + vue_vet_practice (RuleRegistry over facts)
       finalize      DiagnosticFinalizer → vue_vet_core ScanSummary
  -> vue_vet_reporters | vue_vet_lsp | vue_vet_mcp
```

Crate ownership (read before editing that stage):

| Stage | Crate | Notes |
| --- | --- | --- |
| Stable contracts | `vue_vet_core` | facts / diagnostics / `Rule` — no Oxc/Vize types |
| Adapters | `vue_vet_vize`, `vue_vet_oxc` | short-lived AST → facts only |
| Project graph | `vue_vet_project` | see `vue_vet_project` pipeline below |
| Cross-file seeds | `vue_vet_reactivity` | `trace` / `summary` / `link`; `ModuleSummary` boundary; under-approx |
| File rules | `vue_vet_rules`, `vue_vet_practice` | consume facts; practice off score |
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

- **One retained input snapshot per session revision** — initial discovery walks
  and reads each source, manifest, and resolver input once. Cache lookup and a
  cache-miss analysis share that snapshot; `apply_changes` updates only named
  paths. Sources are retained as `Arc<str>`, Nuxt declaration mappings are built
  from the same bytes, and package environments are parsed once into
  `PackageIndex`.
- **Files parallel, pipeline per file sequential** — parse / facts / seed-aware
  rules use Rayon (`--threads N` optional). The same bound is passed to module
  tracing, so `--threads` constrains the complete scan rather than only the
  outer file pass.
- **Rules are pass-based, not “each rule re-scans everything”** — `Rule` exposes
  oxlint-style hooks over Vue Vet facts (not dependency AST):
  - `run_once` — whole-file / cross-fact aggregation
  - `run_on` + `fact_kinds` — per-fact visitor with a bitset interest set
  - `RuleRegistry` runs `run_once` once per rule, then a **single walk** over each
    fact surface (template elements, script calls, reactivity scopes, …) dispatching
    only bucketed interested rules. Rules must report immediately; they must not
    `collect` intermediate vectors and re-scan them.
- **Two-phase bounded module reactivity** — `TraceModulesOptions::max_workers`
  caps both Rayon phases; there is never one native thread per module. Oxc's
  adapter extracts script facts, the local graph, and opaque Vue Vet-owned module
  summaries from one semantic. The coordinator resolves seeds from those
  summaries. Modules without seeds reuse their local graph; a seeded consumer
  reparses only when its source or resolved seed plan changed, while unchanged
  final graphs are reused from `ModuleTraceState`. Oxc arena values never cross
  a thread or adapter boundary and the workspace still forbids `unsafe_code`.
- **Dirty-set scheduling (parse locality)** — `apply_changes` retains the returned
  dirty `FileId` set in `PendingChanges`. `analyze_affected` returns the last
  snapshot when the workspace revision is unchanged (refcount-cheap for shared
  `summary`/`graph`; other snapshot fields may still clone). Otherwise a
  `ChangeImpact` + `DirtyPlan` decide which files re-parse, which need
  environment/rule refresh, and which diagnostics to finalize. Dirty parse is
  real; dirty linking / graph materialization / diagnostic store updates are
  still incomplete — see **Post-#107 locality gap**.
- **Incremental project stages** — `ProjectSession` retains the source snapshot,
  per-file Vize/Oxc facts, raw file diagnostics, structural edge partitions,
  module seed plans/final graphs, and the reverse dependency index. Unrelated
  sources are not re-parsed on a normal edit, but project linking and summary
  rebuild may still walk the full retained set until partition updates land.
- **Atomic session publication** — the workspace revision, retained input
  snapshot, and committed analysis state share one `SessionCore` synchronization
  domain. Analysis captures `Arc` snapshots under the lock, computes outside it,
  and commits only when the same revision is still current. Input mutations are
  transactional (copy-then-commit) and advance the revision in the same critical
  section before releasing that lock.
- **Resolver-context parity** — `ProjectContext.epochs` tracks independent
  counters for package / lockfile / tsconfig / Nuxt / source-membership so
  debounced mutations cannot drop a prior kind. Context changes must not be
  equated with re-parse: tsconfig/lockfile/membership bump resolution and
  indexes; package Vue-version / Nuxt declarations refresh environments, rules,
  and component conventions. File-rule diagnostic reuse still requires matching
  primary and ordinary final module graphs. Incremental results remain equal to
  a clean scan.
- **Shared Rayon pool** — session `--threads N` lazily builds one persistent pool
  on the first real scan (never on warm cache hits) and passes
  `TraceModulesOptions { reuse_current_pool: true }`. Standalone
  `trace_modules_with_options` still installs a dedicated pool sized to
  `max_workers`. `AnalysisSnapshot` shares `summary`/`graph` via `Arc`.
- **Analysis state preparation** — each run seeds a candidate from the previous
  committed state, shares `ProjectGraphState` and file/diagnostic maps via `Arc`
  (copy-on-write / `share_from` on cache hit), and reuses `Arc<AnalyzedCandidate>`
  instead of deep-cloning per-file IR. `ModuleSummary` / `ModuleReactivity.graph` /
  `ScriptBlockFacts.reactivity_graph` share graphs by `Arc`; mutations use
  `Arc::make_mut`. Export resolution uses a worklist rather than cloning the full
  resolved map each fixed-point round. Session input updates fork the snapshot Arc
  once (`Arc::make_mut` + in-place apply), never clone-then-clone.
- **Partial module outcomes** — parse/link failures are scoped
  `AnalysisIssue`s. Healthy modules still reach the cross-module fixed point;
  one bad module never forces every other module back to an isolated local graph.
- **Determinism after concurrency** — diagnostics are sorted in `ScanSummary::finish`;
  module results are sorted by module id after parallel re-trace.
- **Still single-process Rust** — no JS rule host; adapters stay behind Vue Vet facts.
  Facts remain the stable rule surface; the pass walks those facts, not Oxc/Vize nodes.

### Post-#107 locality gap

PR [#107](https://github.com/alexzhang1030/vue-vet/pull/107) proved dirty-set
scheduling and shared IR are the right direction. Batch 1 execution locality is
tracked in [#108](https://github.com/alexzhang1030/vue-vet/issues/108). Current state:

```text
dirty source → fewer parses                    (shipped)
warm linking surface → skip export/seed FP     (shipped)
warm base+facts → reuse template/prop layers   (shipped)
TrackingScopeIR / Vize bottom-up               (shipped)
export-closure seed recompute + SFC blocks     (shipped)
warm disk hit stays cache-load cheap           (shipped; no eager IR hydrate)
returns_by_function + SourceContext            (shipped)
```

Do **not** pursue a generalized unified AST IR. Further locality work belongs in
narrower dirty linking / diagnostics plans, not a new AST layer.

Execution plan shape (session-owned):

```text
ChangeImpact { parse, environment, resolution, component_index, membership }
  → DirtyPlan { parse_files, structural_files, module_summaries,
                export_closure, rule_files, diagnostic_files }
  → stage work counters (files_parsed, partitions rebuilt, COW clones, …)
```

Batch intent (execution lives in tracker issues, not temporary numbers here):

1. **Execution locality** — domain dirty plans; context changes without re-parse;
   work counters; `AnalysisProduct` so LSP publishes diagnostics without the full
   graph DTO (`analyze_affected_product` / `diagnostics_for`).
2. **Project/module state** — `ProjectGraphState` keeps internal Arc partitions
   (structural / module-trace / layered) plus a retained resolver. Linking cache
   skips export/provide/seed fixed points when the linking surface (imports,
   exports, locals, provides, injects — not `local_graph`) and links are
   unchanged; when only some surfaces change, seed plans recompute for the
   export/inject closure. Linking reuse retains `Arc<ModuleSummary>` and prefers
   `Arc::ptr_eq` — never clone a per-module linking-surface map on every scan.
   Layered cache reuses post-template/prop graphs when base graph Arcs and
   `SfcFacts` Arcs are unchanged. Disk-cache hits stay cache-load cheap and must
   not eagerly re-scan; empty session IR is seeded on the first dirty analyze
   via `force_full_parse` (`!has_file_facts()`).
3. **Single-file algorithms** — `TrackingScopeIR`; Vize bottom-up
   `SubtreeSummary`; SFC `SfcBlockRevisions` (style/template/script reuse);
   `returns_by_function` for composable shapes; shared `SourceContext`
   (`Arc<str>` + `Arc<LineIndex>`) at analysis / open-document boundaries.

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

**Vue JSX/TSX** is an Oxc-owned third surface (not Vize): JSX lowers into the
same `TemplateFacts` so template rules and `ComponentUsage` edges reuse without
a parallel pattern engine or Babel transform. Structural JSX facts are collected
only when the script language is `jsx`/`tsx`; `TrackingScopeKind::Render` and
JSX expression joins apply inside recognized render bodies (structure-first
options/`setup`→render / exported functional components, plus same-file
`defineComponent` alias and one-hop identity forwarders). Session runs the Vue
file-rule registry on `.jsx`/`.tsx` (or scripts with non-empty lowered template
facts), not on every plain `.js`/`.ts` module. See issue
[#134](https://github.com/alexzhang1030/vue-vet/issues/134).

`ModuleSummary` (formerly the opaque `PreparedModuleTrace`) is the formal
cross-module boundary: imports, exports, provides/injects, local reactivity, and
no Oxc/Vize nodes. Session file-rule reuse is keyed by `FileRuleInputKey`:
source and `RuleEnvironment` via `content_digest` / `serde_digest`, and final
primary/ordinary module graphs via in-memory `Arc` content equality (avoid
re-serializing full graphs on every file). Rule-level semantic views
(`EffectModel`, …) are deferred until multiple rules repeat the same derivation.

`no-v-html` remains the reference AST-backed built-in rule. Phase 2 adds the Oxc
adapter while keeping both dependency ASTs behind Vue Vet-owned facts.
Every built-in lint rule is a self-contained module under `vue_vet_rules/src/rules`:
the module owns its metadata, rule type, and detection/reporting logic. The
parent module only declares modules and assembles the built-in registry; it does
not dispatch rule behavior through a shared enum or central match.
Ecosystem and migration practice suggestions live in `vue_vet_practice`: recipe
metadata plus thin `Rule` implementations that consume the same Vue Vet facts
(no parallel pattern engine). Practice findings use `category: "practice"`,
attach an optional `recommendation` payload, and stay off the score / default CI
exit path. Some practice rules keep a historical rule id segment (for example
`vue-vet/reactivity/prefer-use-template-ref`) for configuration stability.
Tracking-graph / after-await registrar packs may live as a **matrix family** under
`vue_vet_rules/src/rules/matrix` (shared detection type + unique `RuleMeta` catalog);
standalone rules remain one file each. Matrix ids still ship docs and fixtures.
The session derives per-file Vue capabilities from a single discovery-time
`PackageIndex` (nearest `package.json`: `vue` version plus dependency names) and passes them in
`RuleEnvironment` without exposing package-manager state to parser adapters.
Practice recipes may adjust help text when `@vueuse/core` is already declared.
The Oxc adapter delegates reactivity construction to `vue_vet_reactivity`.
That crate is the static reactivity tracing library: it records Vue-resolved
bindings and **tracking scopes** (`watchEffect*`, `computed`, `watch` sources)
as serializable Vue Vet facts. Each scope carries demand reads with property,
exact span, classification (unconditional, conditional, after-await, or
outside-tracking), and ordered guard evidence with roles; rules never receive
Oxc nodes. Legacy `effects` is a projection of effect-family scopes for existing
consumers. Its module layer summarizes direct bindings and composable return
shapes (destructure and instance member seeds), then reaches a deterministic
fixed point over resolved named/default exports, barrels, multi-hop re-exports,
and cycles. See [reactivity tracer](./reactivity-tracer.md).
Configuration changes
rule enablement and severity after semantic analysis;
suppressions are applied after diagnostic normalization and emit findings when
unused.

## Stable boundary

Vue Vet's normalized facts and diagnostics are the architectural seam. Dependency AST objects must not cross into public rule, reporter, cache, LSP, or agent contracts. Adapters may change with dependency upgrades while downstream product behavior stays versioned and reviewable.

## `vue_vet_project` pipeline (crate layout)

The project crate is an **explicit stage pipeline**, not a monolith with
side-pocket special cases. `lib.rs` is a thin façade; orchestration lives in
`pipeline.rs`:

```text
context          ConventionsLoad → ProjectContext
structural       StructuralLink (import/component edges)
passes           enrichment (see below)
pipeline         Trace handoff + ProjectGraph assembly
layers           template joins + prop-flow
rules            unresolved-import / unused-component
model / state    DTOs + retained incremental partitions
resolve / conventions   oxc_resolver + Nuxt maps
```

### Analysis enrichment passes (not user plugins)

Nuxt / package-shape specialization lives in **compile-time Rust enrichment
passes** over Vue Vet IR — not AST Traverse (Oxc/SWC), and not a dynamic JS
plugin host. Diagnostic [`Rule`](../../crates/vue_vet_core/src/lib.rs) passes
consume the enriched facts; enrichment passes must not `report` diagnostics.

### Reactivity tracer plugins (`vue_vet_plugins`)

Ecosystem **named API bag** contracts (Nuxt `useAsyncData` / `useFetch`, vue-i18n
`useI18n` ambient-on-call methods, …) are **not** hardcoded inside
`vue_vet_reactivity`. The engine accepts a [`NamedApiBag`] catalog via
`TraceConfig` / `TraceModulesOptions`. The **published** `vue_vet_plugins` crate
implements [`TracerPlugin`] and exposes `default_named_api_bags()` /
`default_trace_config()` / `ensure_default_plugins()`.

**Auto-load:** Oxc single-file analysis, `vue_vet_project` graph builds, and
`vue_vet_session` (CLI / LSP / MCP) install the default catalog at the boundary
so product scans always see Nuxt / vue-i18n modeling. Pure `vue_vet_reactivity`
callers stay empty until they depend on `vue_vet_plugins` and pass a catalog.

Still compile-time Rust only — no `dlopen` / npm plugin ABI. crates.io publish
order: `vue_vet_core` → `vue_vet_reactivity` → `vue_vet_plugins`.

Each enrichment step is a named `struct` with an inherent `::run(...)`
(see `ENRICHMENT_STEPS` in `vue_vet_project::passes`). There is no empty
metadata trait and no dynamic plugin ABI — `pipeline` / `structural` call
passes by name.

Enrichment stages (deterministic order):

```text
ConventionsLoad           (context + conventions → ProjectContext maps)
  -> StructuralLink       (structural.rs ordinary edges;
                           NuxtImportsSeedPass::run for bare auto-imports)
  -> ExternalSummaryLoad  (ExternalSummaryLoadPass::run)
       └─ SummaryMerge    (ProvisionalFactoryMergePass::run at each loaded
                           module — same traversal, not a hidden side effect)
```

After enrichment: SeedPlan / Trace (via `vue_vet_reactivity` from `pipeline`)
and RuleRegistry (file rules outside this crate). Project-level diagnostics
stay in `rules.rs`.

Constraints: IR only (`ProjectContext`, `ModuleLink`, `ModuleSummary`,
`ExportState`); sorted outputs; quiet under-approx; no `dlopen` / npm analysis
plugins before a separate ADR. See [gotchas](./gotchas.md) and
[reactivity tracer](./reactivity-tracer.md).

## Planned analysis flow

```text
project discovery and configuration
  -> Vize SFC/template facts
  -> Oxc script facts
  -> enrichment passes (Nuxt seeds, external summaries, provisional Factory merge)
  -> per-file built-in rules
  -> versioned project graph and graph-backed cross-file rules
  -> normalize, suppress, deduplicate, fingerprint
  -> content-addressed normalized-result cache
  -> score, baseline/diff, report, preview/apply fixes
```

## Crate evolution

Existing crates are `vue_vet_core`, `vue_vet_config`, `vue_vet_vize`,
`vue_vet_oxc`, `vue_vet_reactivity`, `vue_vet_rules`, `vue_vet_practice`,
`vue_vet_project`, `vue_vet_reporters`, `vue_vet_session`, and the `vue-vet` CLI.
New rule capabilities extend these semantic and product boundaries only when a
working vertical slice exercises them; there is no separate pattern-engine
boundary in the roadmap.

`vue_vet_session` owns the long-lived project analysis handle: config load,
cached/fresh scans, unsaved overlays, per-file fact state, reverse dependencies,
rule/finding explain, and workspace path containment. `apply_changes` plus
`analyze_affected` schedule from `ChangeImpact`/`DirtyPlan`: reparses only
parse-dirty files, refreshes environments/rules when context demands it, reuses
unchanged facts and file-rule results when keys match, and expands graph
consumers through the reverse index. Structural/module partitions still often
rebuild broadly until Batch 2; work counters expose that cost. Overlay analysis
bypasses the content-addressed cache. A file or module failure becomes a scoped
`AnalysisIssue` while healthy files and module links continue; fatal root or
configuration errors still fail the request. The CLI and `vue_vet_lsp` consume the session so
diagnostic identity stays shared across surfaces. The thin LSP (`vue-vet --lsp`)
publishes diagnostics on `didOpen` / `didChange` / `didSave` from open-buffer
overlays (FULL sync) with the opaque finding id in LSP `data` and the document
version on `publishDiagnostics`. Overlay changes advance a workspace revision
in the same critical section that updates the retained input snapshot. A 50 ms
debounce and single latest-wins gate admit only the newest blocking task; stale
work cancels between pipeline phases and its commit is rejected under the same
session lock. The resulting snapshot refreshes every open document. Safe
quick-fix code actions return versioned
workspace edits from explicitly safe diagnostic edits only (client applies;
server never writes). The thin MCP adapter (`vue-vet --mcp`, `vue_vet_mcp`)
exposes scan / explain / explain-scope / safe-fix preview tools over stdio
JSON-RPC with the same session path bounds; MCP never applies edits.

### Published library crates

`vue_vet_core` and `vue_vet_reactivity` are the first crates intended for
crates.io. Goals: reserve the names, expose the stable fact / tracer contracts
to external consumers, and keep the rest of the workspace (`publish = false`)
until the CLI and adapters have a deliberate release story. Published packages
omit in-tree fixtures and the runtime oracle; those remain git-only evidence.
Path dependencies between publishable crates carry an explicit `version` so
`cargo publish` can resolve them from the registry. Crate directories and package
names use snake_case (see [conventions](./conventions.md)).

Tagged releases publish those two crates automatically from
`.github/workflows/release.yml` (after quality gates; `vue_vet_core` then
`vue_vet_reactivity`) using the `CARGO_REGISTRY_TOKEN` repository secret.
Index polls use a descriptive User-Agent and skip versions already on the
registry so a partial publish can resume. See [install docs](../../docs/install.md)
and [gotchas](./gotchas.md) (`crates.io API calls need a User-Agent`).

### Native binary and npm distribution

End-user installs go through npm (`@vue-vet/cli` + `@vue-vet/*` platform
packages) or GitHub Release archives, not crates.io for the CLI
(`publish = false`). The Release workflow (`.github/workflows/release.yml`)
publishes library crates, builds the matrix targets, writes `SHA256SUMS`,
publishes platform packages, then the launcher. Version numbers stay aligned
across Cargo workspace, npm, and `v*` tags. Details: [install docs](../../docs/install.md).

## Reporting and edit planning

Explain domain models live in `vue_vet_core`/`vue_vet_session`; reporters do not
own session state or domain construction. `vue_vet_reporters` consumes Vue
Vet-owned `ScanSummary` values plus an explicit
report context for scan mode, framework, exact analyzed files, completeness, and
skipped-check reasons. It owns deterministic text and versioned JSON rendering,
while the CLI retains stdout, operational-error messages, and exit policy.
`ReportContext.color` is injected by the CLI (`--color auto|always|never`); only
the interactive text report applies ANSI styles. JSON / SARIF / GitHub stay
uncolored. Renderers return content without a terminal newline so each surface
can choose its transport framing. Text snapshots remain byte-for-byte
compatibility gates (color off); JSON snapshots are versioned wire-contract
gates.

JSON v1 is the shared fact layer for CI and future agent surfaces. Each finding
has a deterministic opaque ID, normalized project-relative path, confidence,
and repository-local documentation path. Consumers must use `complete` and exact
analyzed-file coverage rather than treating an empty findings array as proof of
a clean scan. A future bounded agent handoff may summarize and group this data,
but it must reference the complete report instead of replacing it.

The shared edit contract lives in `vue_vet_core`, not in a parser, rule engine,
or reporter. A text edit carries a repository path, checked byte range,
replacement, safe/unsafe applicability, and originating rule ID. `EditPlan`
normalizes ordering and rejects range overflow, overlapping replacements, and
order-dependent insertions. An active diagnostic may carry edit candidates so
configuration and suppression remove the finding and its edits together. JSON
v1 exposes those candidates as an optional field without changing diagnostic
identity.

The CLI's private fix module has one interface for previewing or applying active
safe edits. It resolves repository-relative targets inside the scan scope,
consumes `EditPlan`, validates source bounds and UTF-8 boundaries, and applies
one file from the original source in reverse-range order. Apply mode uses a
same-directory atomic replacement and then performs a fresh scan; both fix modes
bypass cached results so a persisted plan can never authorize mutation. This
first vertical slice fails closed on multi-file plans. Cross-file staging,
rollback, and further producers remain later issue #9 work. Shipped single-file
producers: boolean `autofocus` removal, quoted `aria-hidden="true"` /
`:aria-hidden="true"` removal on focusable elements, static `title` →
`aria-label` inserts, and redundant static `role` removal.

## Identity and determinism

Rule IDs and diagnostic fingerprints must remain stable enough for baselines,
diff mode, SARIF, LSP, and agent consumers. Results are sorted independently of
traversal or hash-map order. Discovery converts physical paths exactly once to
workspace-relative normalized `FileId`; diagnostics, edits, graphs, caches,
baselines, LSP, and reporters compare that identity exactly. Suffix matching is
forbidden. Physical paths stay in the source/I/O adapter. Coverage reports
analyzed source files separately from manifests, lockfiles, and resolver inputs
that invalidate the graph.

## Thin editor host and diagnostics LSP

`editors/vscode` is a **thin** VS Code host for reactivity visualization. It
spawns the Rust CLI (`--format json --print-reactivity`), maps structured
`*_details` byte spans onto decorations / hover / a TreeView, and must not grow
a parallel tracer. **Explain Scope** shells `vue-vet --explain-scope @offset`
(covering fallback) and paints `ScopeExplain`; hover may show
`scope_details[].summary` from the digest.

`vue-vet --lsp` is the diagnostics LSP surface (`vue_vet_lsp`). It uses
`vue_vet_session` with open-buffer overlays and publishes
`textDocument/publishDiagnostics` with the same opaque finding ids as JSON
`diagnostics[].id` (stored in LSP `data`) plus the document version. Safe
quick-fix code actions map active safe edits to versioned `WorkspaceEdit`s.
`vue-vet --mcp` (`vue_vet_mcp`) exposes stdio JSON-RPC tools for scan, explain,
explain-scope (`vue_vet_explain_scope`, same `ScopeExplain` JSON as CLI
`--explain-scope`), and safe-fix preview with the same workspace path bounds;
it never applies edits. Request-level cancellation remains later issue #12 work.

## Project intelligence

Cross-file findings are derived from a Vue Vet-owned graph of imports, components, composables, routes, stores, and Nuxt conventions. Diff mode must invalidate and re-run affected graph consumers; it cannot scan only changed files and silently lose a newly caused project-level failure.

The first graph layer is `vue_vet_project`. It consumes serializable `SfcFacts`,
uses repository-relative file IDs, stores source evidence on every edge, and
publishes its exact file inputs for cache invalidation. Its convention version
changes whenever Nuxt directory or naming behavior changes. The project graph
also supplies resolved module edges (standalone JS/TS **and** preferred SFC
script blocks) to `vue_vet_reactivity` and publishes the resulting per-module
graphs. Extracted `.vue` scripts use Vize block offsets plus the original SFC
as `span_source` so absolute spans stay exact; template joins are re-applied on
the module graph after cross-file seed linking.

Cache format version 3 stores only `ScanSummary` and `ProjectGraph`, including
rule confidence, documentation metadata, and optional edit candidates on cached
diagnostics. Its key includes every source body plus configuration, tool,
dependency, convention, and ruleset versions. Baseline filtering and diff
filtering happen after cache lookup so those presentation choices do not
fragment semantic cache entries.
Fix modes still force a fresh scan before planning.

See [technology stack](./technology-stack.md), [conventions](./conventions.md), and [the roadmap](../../ROADMAP.md).
