# Technology stack decisions

## Rust is the implementation language

The scanner, semantic product layer, CLI, cache, graph, reporters, and fix engine stay in Rust. The future npm package is a thin installer/launcher whose only responsibilities are selecting the native binary and forwarding arguments, signals, output, and exit codes.

The workspace tracks the latest stable Rust release and latest stable edition, following Rolldown's toolchain baseline. The repository pins the exact compiler in `rust-toolchain.toml`; all crates inherit the workspace `rust-version` and edition. Rolldown's lint policy is a floor: Vue Vet additionally denies the Clippy `all`, `cargo`, `pedantic`, and `nursery` groups, forbids unsafe Rust, and denies panic-prone conveniences such as unchecked indexing, string slicing, `unwrap`, and `expect`. The group-level duplicate-version exception covers the reviewed Vize and atomic-writer dependency graphs; each additional duplicate still requires explicit rationale. `just` is the task runner and the canonical interface for local and CI validation. `prek` manages Git hooks from `.pre-commit-config.yaml` without adding a Python runtime requirement.

## Vize owns Vue semantics

Vize parses Vue SFC structure and is the source of truth for template nodes, directives, bindings, and Vue-specific semantics. Vue Vet adapts Vize output into its own stable facts rather than exposing Vize AST types. Vize is experimental and releases rapidly, so its version is pinned exactly and upgrades require fixtures, snapshots, and compatibility evidence.

## Oxc owns JavaScript and TypeScript semantics

Oxc will parse script blocks and provide scopes, symbols, references, imports, and expression facts. Vue Vet is not initially a complete TypeScript type checker; rules requiring unavailable type certainty must remain out of the default preset or explicitly experimental.

The first adapter is pinned to the Oxc 0.127 family already present in Vize's
locked dependency graph. It parses each extracted script surface using its
declared `lang`, builds semantics with syntax checking, and maps every fact span
back through the SFC block offset. Direct Oxc types remain private to
`vue-vet-oxc`.

## Vize and Oxc are the complete analysis stack

Vue Vet does not embed a parallel structural-pattern engine. Template and SFC
diagnostics extend Vize-backed facts; JavaScript and TypeScript diagnostics
extend Oxc-backed facts. This keeps source offsets, confidence, suppression,
scoring, caching, baselines, and fixes on one semantic path. Teams that need
repository-specific structural conventions can run standalone search tools in
CI without making their grammar and rule contracts part of Vue Vet's stable
product surface.

## atomic-write-file owns the single-file commit

The safe-fix executor uses exact-pinned `atomic-write-file` 0.3.0 for the final
same-directory replacement on Unix, Windows, and WASI. Vue Vet still owns edit
classification, planning, scan-scope containment, byte/UTF-8 validation, and
post-fix rescanning; the dependency receives only a completely rendered file
body and owns the filesystem-specific atomic commit. It does not provide or
imply a multi-file transaction. Its `nix` and `rand` versions are a reviewed
source of duplicate transitive packages rather than a reason to weaken the
workspace lint policy further.

## Vue Vet owns the product contracts

Diagnostics, source spans, rule metadata, confidence, configuration, suppression, fingerprints, scoring, project facts, cache formats, baselines, reporters, and edits are Vue Vet-owned types. This boundary prevents dependency churn from becoming a public API break.

See [the analysis-stack ADR](../../docs/adr/0001-analysis-stack.md) for the decision and [architecture](./architecture.md) for the planned data flow.
