# Architecture

## Current vertical slice

```text
vue-vet CLI
  -> vue_vet_session (config, cache, scan, explain, workspace paths)
       -> ignore-aware .vue / JS / TS discovery (sequential walk)
       -> parallel per-file facts (Vize SFC / Oxc modules)   // oxlint-style
       -> vue_vet_project edges + vue_vet_reactivity module seed linking
          (module first-pass + seeded re-trace also parallel)
       -> apply module graphs onto setup and dual ordinary (#script) blocks
       -> parallel seed-aware vue_vet_rules + vue_vet_practice
       -> severity overrides and scoped suppressions
       -> vue_vet_core diagnostics, spans, scoring (sorted for determinism;
          category `practice` excluded from score / default CI exit)
  -> vue_vet_reporters text or JSON rendering
  -> CLI output and CI exit policy
```

### Performance model (oxlint-inspired)

- **Files parallel, pipeline per file sequential** — discovery is sequential; parse /
  facts / seed-aware rules use Rayon (`--threads N` optional).
- **Rules are pass-based, not “each rule re-scans everything”** — `Rule` exposes
  oxlint-style hooks over Vue Vet facts (not dependency AST):
  - `run_once` — whole-file / cross-fact aggregation
  - `run_on` + `fact_kinds` — per-fact visitor with a bitset interest set
  - `RuleRegistry` runs `run_once` once per rule, then a **single walk** over each
    fact surface (template elements, script calls, reactivity scopes, …) dispatching
    only bucketed interested rules. Rules must report immediately; they must not
    `collect` intermediate vectors and re-scan them.
- **Two-phase module reactivity, one parse** — sticky workers (`std::thread::scope`)
  keep each module's Oxc allocator/semantic on the worker stack. Phase 1 builds
  export shapes and an empty-seed local graph; after the coordinator resolves
  seeds, phase 2 re-traces on the **same** semantic when cross-file seeds exist
  (no second parse). Sessions never leave their thread (arena types are not
  `Send`; the workspace forbids `unsafe_code`). Results stay deterministic after
  sort-by-module-id.
- **Determinism after concurrency** — diagnostics are sorted in `ScanSummary::finish`;
  module results are sorted by module id after parallel re-trace.
- **Still single-process Rust** — no JS rule host; adapters stay behind Vue Vet facts.
  Facts remain the stable rule surface; the pass walks those facts, not Oxc/Vize nodes.

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
The CLI/session derives per-file Vue capabilities from the nearest package.json
(`vue` version plus dependency package names) and passes them in
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

## Planned analysis flow

```text
project discovery and configuration
  -> Vize SFC/template facts
  -> Oxc script facts
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
cached/fresh scans, unsaved buffer overlays (`analyze_with_overlays`), rule and
finding explain, and workspace path containment. Overlay analysis always bypasses
the content-addressed cache. The CLI and `vue_vet_lsp` consume the session so
diagnostic identity stays shared across surfaces. The thin LSP (`vue-vet --lsp`)
publishes diagnostics on `didOpen` / `didChange` / `didSave` from open-buffer
overlays (FULL sync) with the opaque finding id in LSP `data` and the document
version on `publishDiagnostics`. Overlapping overlay analyses are dropped via
per-document generation tokens. Safe quick-fix code actions return versioned
workspace edits from explicitly safe diagnostic edits only (client applies;
server never writes). The thin MCP adapter (`vue-vet --mcp`, `vue_vet_mcp`)
exposes scan / explain / safe-fix preview tools over stdio JSON-RPC with the
same session path bounds; MCP never applies edits. Request-level cancellation
remains later issue #12 work.

### Published library crates

`vue_vet_core` and `vue_vet_reactivity` are the first crates intended for
crates.io. Goals: reserve the names, expose the stable fact / tracer contracts
to external consumers, and keep the rest of the workspace (`publish = false`)
until the CLI and adapters have a deliberate release story. Published packages
omit in-tree fixtures and the runtime oracle; those remain git-only evidence.
Path dependencies between publishable crates carry an explicit `version` so
`cargo publish` can resolve them from the registry. Crate directories and package
names use snake_case (see [conventions](./conventions.md)).

### Native binary and npm distribution

End-user installs go through npm (`@vue-vet/cli` + `@vue-vet/*` platform
packages) or GitHub Release archives, not crates.io for the CLI
(`publish = false`). The Release workflow (`.github/workflows/release.yml`)
builds the matrix targets, writes `SHA256SUMS`, publishes platform packages,
then the launcher. Version numbers stay aligned across Cargo workspace, npm,
and `v*` tags. Details: [install docs](../../docs/install.md).

## Reporting and edit planning

`vue_vet_reporters` consumes Vue Vet-owned `ScanSummary` values plus an explicit
report context for scan mode, framework, exact analyzed files, completeness, and
skipped-check reasons. It owns deterministic text and versioned JSON rendering,
while the CLI retains stdout, operational-error messages, and exit policy.
Renderers return content without a terminal newline so each surface can choose
its transport framing. Text snapshots remain byte-for-byte compatibility gates;
JSON snapshots are versioned wire-contract gates.

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
rollback, and more edit producers remain later issue #9 work.

## Identity and determinism

Rule IDs and diagnostic fingerprints must remain stable enough for baselines, diff mode, SARIF, LSP, and agent consumers. Results are sorted independently of traversal or hash-map order. Paths in persisted or machine-readable output are repository-relative and normalized.

## Thin editor host and diagnostics LSP

`editors/vscode` is a **thin** VS Code host for reactivity visualization. It
spawns the Rust CLI (`--format json --print-reactivity`), maps structured
`*_details` byte spans onto decorations / hover / a TreeView, and must not grow
a parallel tracer.

`vue-vet --lsp` is the diagnostics LSP surface (`vue_vet_lsp`). It uses
`vue_vet_session` with open-buffer overlays and publishes
`textDocument/publishDiagnostics` with the same opaque finding ids as JSON
`diagnostics[].id` (stored in LSP `data`) plus the document version. Safe
quick-fix code actions map active safe edits to versioned `WorkspaceEdit`s.
`vue-vet --mcp` (`vue_vet_mcp`) exposes stdio JSON-RPC tools for scan, explain,
and safe-fix preview with the same workspace path bounds; it never applies
edits. Request-level cancellation remains later issue #12 work.

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
