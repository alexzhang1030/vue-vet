# Architecture

## Current vertical slice

```text
vue-vet CLI
  -> ignore-aware .vue discovery
  -> vue-vet-vize SFC parsing and analysis
  -> vue-vet-core diagnostics, spans, scoring
  -> text or JSON output and CI exit policy
```

The current implementation is intentionally small. The existing `no-v-html` attribute scanner is temporary and is scheduled to become the reference Vize AST-backed built-in rule in issue #2.

## Stable boundary

Vue Vet's normalized facts and diagnostics are the architectural seam. Dependency AST objects must not cross into public rule, reporter, cache, LSP, or agent contracts. Adapters may change with dependency upgrades while downstream product behavior stays versioned and reviewable.

## Planned analysis flow

```text
project discovery and configuration
  -> Vize SFC/template facts
  -> Oxc script facts
  -> per-file built-in rules
  -> project graph and cross-file rules
  -> ast-grep custom rules
  -> normalize, suppress, deduplicate, fingerprint
  -> score, baseline/diff, report, preview/apply fixes
```

## Crate evolution

Existing crates are `vue-vet-core`, `vue-vet-vize`, and the `vue-vet` CLI. Planned boundaries include `vue-vet-oxc`, `vue-vet-rules`, `vue-vet-patterns`, `vue-vet-project`, and `vue-vet-reporters`. A planned name is not authorization to create an empty abstraction: introduce the crate only when a working vertical slice uses it.

## Identity and determinism

Rule IDs and diagnostic fingerprints must remain stable enough for baselines, diff mode, SARIF, LSP, and agent consumers. Results are sorted independently of traversal or hash-map order. Paths in persisted or machine-readable output are repository-relative and normalized.

## Project intelligence

Cross-file findings are derived from a Vue Vet-owned graph of imports, components, composables, routes, stores, and Nuxt conventions. Diff mode must invalidate and re-run affected graph consumers; it cannot scan only changed files and silently lose a newly caused project-level failure.

See [technology stack](./technology-stack.md), [conventions](./conventions.md), and [the roadmap](../../ROADMAP.md).

