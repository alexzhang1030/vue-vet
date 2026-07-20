# Architecture

## Current vertical slice

```text
vue-vet CLI
  -> versioned configuration and path filters
  -> ignore-aware .vue discovery
  -> vue-vet-vize SFC and template AST parsing
  -> Vue Vet-owned template facts
  -> vue-vet-oxc script parsing and semantic analysis
  -> Vue Vet-owned script imports, bindings, calls, writes, and destructures
  -> vue-vet-reactivity local tracing and module-summary linking
  -> vue-vet-project resolved edges and module reactivity graphs
  -> vue-vet-rules built-in rule registry
  -> severity overrides and scoped suppressions
  -> vue-vet-core diagnostics, spans, scoring
  -> vue-vet-reporters text or JSON rendering
  -> CLI output and CI exit policy
```

`no-v-html` remains the reference AST-backed built-in rule. Phase 2 adds the Oxc
adapter while keeping both dependency ASTs behind Vue Vet-owned facts.
Every built-in rule is a self-contained module under `vue-vet-rules/src/rules`:
the module owns its metadata, rule type, and detection/reporting logic. The
parent module only declares modules and assembles the built-in registry; it does
not dispatch rule behavior through a shared enum or central match.
The CLI derives per-file Vue capabilities from the nearest package.json and passes
them into per-file rules without exposing package-manager state to parser adapters.
The Oxc adapter delegates reactivity construction to `vue-vet-reactivity`.
That crate records Vue-resolved reactive binding nodes and every direct effect
read as serializable Vue Vet facts. Read edges carry their property, exact span,
classification (unconditional, conditional, or after-await), and ordered guard
evidence; rules never receive Oxc nodes. Its module layer summarizes direct
bindings and composable return shapes, then reaches a deterministic fixed point
over resolved named/default exports, barrels, multi-hop re-exports, and cycles.
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

Existing crates are `vue-vet-core`, `vue-vet-config`, `vue-vet-vize`,
`vue-vet-oxc`, `vue-vet-reactivity`, `vue-vet-rules`, `vue-vet-project`,
`vue-vet-reporters`, and the `vue-vet` CLI. New rule capabilities extend these
semantic and product boundaries only when a working vertical slice exercises
them; there is no separate pattern-engine boundary in the roadmap.

## Reporting and edit planning

`vue-vet-reporters` consumes Vue Vet-owned `ScanSummary` values plus an explicit
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

The shared edit contract lives in `vue-vet-core`, not in a parser, rule engine,
or reporter. A text edit carries a repository path, checked byte range,
replacement, safe/unsafe applicability, and originating rule ID. `EditPlan`
normalizes ordering and rejects range overflow, overlapping replacements, and
order-dependent insertions. It deliberately has no file-application API; disk
mutation, rollback, and post-fix rescans belong to later issue #9 slices.

## Identity and determinism

Rule IDs and diagnostic fingerprints must remain stable enough for baselines, diff mode, SARIF, LSP, and agent consumers. Results are sorted independently of traversal or hash-map order. Paths in persisted or machine-readable output are repository-relative and normalized.

## Project intelligence

Cross-file findings are derived from a Vue Vet-owned graph of imports, components, composables, routes, stores, and Nuxt conventions. Diff mode must invalidate and re-run affected graph consumers; it cannot scan only changed files and silently lose a newly caused project-level failure.

The first graph layer is `vue-vet-project`. It consumes serializable `SfcFacts`,
uses repository-relative file IDs, stores source evidence on every edge, and
publishes its exact file inputs for cache invalidation. Its convention version
changes whenever Nuxt directory or naming behavior changes. The project graph
also supplies resolved standalone JavaScript/TypeScript module edges to
`vue-vet-reactivity` and publishes the resulting per-module graphs. Cross-file
tracing for extracted `.vue` script blocks is intentionally not inferred yet:
it requires a Vize-owned source/offset handoff so SFC spans remain exact.

Cache format version 2 stores only `ScanSummary` and `ProjectGraph`, including
rule confidence and documentation metadata on cached diagnostics. Its key
includes every source body plus configuration, tool, dependency, convention,
and ruleset versions. Baseline filtering and diff filtering happen after cache
lookup so those presentation choices do not fragment semantic cache entries.

See [technology stack](./technology-stack.md), [conventions](./conventions.md), and [the roadmap](../../ROADMAP.md).
