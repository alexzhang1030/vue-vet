# ADR 0001: Vize semantics with an ast-grep extension lane

Status: accepted

## Decision

Vue Vet is a Rust workspace. Vize owns Vue SFC parsing and Vue template
semantics. Oxc owns JavaScript/TypeScript syntax and semantic analysis.
ast-grep provides opt-in declarative pattern rules.

The stable boundary is Vue Vet's own diagnostic model. Neither Vize nor
ast-grep types are exposed by the CLI JSON contract.

## Why ast-grep is not the primary engine

Pattern matching is excellent for local shapes and team-specific conventions.
A doctor also needs binding resolution, Vue reactivity knowledge, component
graphs, confidence ranking, deduplication, baselines, scoring, and coordinated
fixes. Those capabilities require a semantic product layer.

## Consequences

- Vize is pinned because its public API is still evolving.
- Built-in rules may use deeper semantic APIs than user-authored rules.
- Custom ast-grep rules can participate in the same scoring and reporting
  pipeline but must declare their language surface and severity.
- Every dependency upgrade needs golden SFC fixtures and diagnostic snapshots.

The executable Vize compatibility assumptions and upgrade checklist live in
[the Vize compatibility baseline](../vize-compatibility.md).
