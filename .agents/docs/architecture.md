# Architecture

## Current vertical slice

```text
vue-vet CLI
  -> versioned configuration and path filters
  -> ignore-aware .vue discovery
  -> vue-vet-vize SFC and template AST parsing
  -> Vue Vet-owned template facts
  -> vue-vet-oxc script parsing and semantic analysis
  -> Vue Vet-owned script imports, bindings, calls, writes, destructures, and reactivity graph facts
  -> vue-vet-rules built-in rule registry
  -> severity overrides and scoped suppressions
  -> vue-vet-core diagnostics, spans, scoring
  -> text or JSON output and CI exit policy
```

`no-v-html` remains the reference AST-backed built-in rule. Phase 2 adds the Oxc
adapter while keeping both dependency ASTs behind Vue Vet-owned facts.
The CLI derives per-file Vue capabilities from the nearest package.json and passes
them into per-file rules without exposing package-manager state to parser adapters.
The Oxc adapter records reactive binding nodes and conditional effect-read edges as
serializable Vue Vet facts; rules never receive Oxc nodes. Configuration changes
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
  -> ast-grep custom rules
  -> normalize, suppress, deduplicate, fingerprint
  -> content-addressed normalized-result cache
  -> score, baseline/diff, report, preview/apply fixes
```

## Crate evolution

Existing crates are `vue-vet-core`, `vue-vet-config`, `vue-vet-vize`,
`vue-vet-oxc`, `vue-vet-rules`, and the `vue-vet` CLI. Planned boundaries
include `vue-vet-patterns`, `vue-vet-project`, and `vue-vet-reporters`. A planned name is not authorization
to create an empty abstraction: introduce the crate only when a working vertical
slice uses it.

## Identity and determinism

Rule IDs and diagnostic fingerprints must remain stable enough for baselines, diff mode, SARIF, LSP, and agent consumers. Results are sorted independently of traversal or hash-map order. Paths in persisted or machine-readable output are repository-relative and normalized.

## Project intelligence

Cross-file findings are derived from a Vue Vet-owned graph of imports, components, composables, routes, stores, and Nuxt conventions. Diff mode must invalidate and re-run affected graph consumers; it cannot scan only changed files and silently lose a newly caused project-level failure.

The first graph layer is `vue-vet-project`. It consumes serializable `SfcFacts`,
uses repository-relative file IDs, stores source evidence on every edge, and
publishes its exact file inputs for cache invalidation. Its convention version
changes whenever Nuxt directory or naming behavior changes.

Cache format version 1 stores only `ScanSummary` and `ProjectGraph`. Its key
includes every source body plus configuration, tool, dependency, convention,
and ruleset versions. Baseline filtering and diff filtering happen after cache
lookup so those presentation choices do not fragment semantic cache entries.

See [technology stack](./technology-stack.md), [conventions](./conventions.md), and [the roadmap](../../ROADMAP.md).
