# Technology stack decisions

## Rust is the implementation language

The scanner, semantic product layer, CLI, cache, graph, reporters, and fix engine stay in Rust. The future npm package is a thin installer/launcher whose only responsibilities are selecting the native binary and forwarding arguments, signals, output, and exit codes.

## Vize owns Vue semantics

Vize parses Vue SFC structure and is the source of truth for template nodes, directives, bindings, and Vue-specific semantics. Vue Vet adapts Vize output into its own stable facts rather than exposing Vize AST types. Vize is experimental and releases rapidly, so its version is pinned exactly and upgrades require fixtures, snapshots, and compatibility evidence.

## Oxc owns JavaScript and TypeScript semantics

Oxc will parse script blocks and provide scopes, symbols, references, imports, and expression facts. Vue Vet is not initially a complete TypeScript type checker; rules requiring unavailable type certainty must remain out of the default preset or explicitly experimental.

## ast-grep is the extension lane

ast-grep will power YAML-defined structural rules for supported script and template surfaces. It is a fast and useful customization mechanism, but it does not own built-in Vue semantics, cross-file project reasoning, confidence, scoring, suppression, caching, baselines, or fix coordination.

## Vue Vet owns the product contracts

Diagnostics, source spans, rule metadata, confidence, configuration, suppression, fingerprints, scoring, project facts, cache formats, baselines, reporters, and edits are Vue Vet-owned types. This boundary prevents dependency churn from becoming a public API break.

See [the analysis-stack ADR](../../docs/adr/0001-analysis-stack.md) for the original decision and [architecture](./architecture.md) for the planned data flow.

