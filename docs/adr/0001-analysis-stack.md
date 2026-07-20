# ADR 0001: Vize and Oxc as the semantic analysis stack

Status: accepted

## Decision

Vue Vet is a Rust workspace. Vize owns Vue SFC parsing and Vue template
semantics. Oxc owns JavaScript/TypeScript syntax and semantic analysis. New
rules extend Vue Vet-owned facts produced by those adapters; Vue Vet does not
embed a second structural-pattern engine.

The stable boundary is Vue Vet's own diagnostic model. Neither Vize nor
Oxc types are exposed by the CLI JSON contract.

## Why the analysis stack stays semantic

Vue Vet prioritizes high-confidence Vue and Nuxt diagnostics. Binding
resolution, reactivity knowledge, component graphs, confidence ranking,
baselines, scoring, and coordinated fixes depend on shared semantic facts.
Embedding a separate pattern engine would add another parser and rule model,
SFC offset mapping, overlap policy, dependency compatibility surface, and
resource budget before project demand for an integrated custom-rule surface
has been demonstrated. Teams can run standalone structural-search tools beside
Vue Vet when they need repository-specific conventions.

## Consequences

- Vize is pinned because its public API is still evolving.
- Oxc stays behind a dependency-neutral adapter just like Vize.
- Rules consume Vue Vet-owned semantic facts and project graph evidence.
- A future customization mechanism requires a separate product decision backed
  by concrete user demand; it is not part of the current roadmap.
- Every Vize or Oxc upgrade needs compatibility fixtures and diagnostic snapshots.

The executable Vize compatibility assumptions and upgrade checklist live in
[the Vize compatibility baseline](../vize-compatibility.md).
