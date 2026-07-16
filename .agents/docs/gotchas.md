# Known gotchas

## Vize API churn is expected

Vize is not yet production-stable and publishes frequently. Keep the dependency exact-pinned. An upgrade is a compatibility task: compile, inspect API changes, run golden fixtures and diagnostic snapshots, and record behavior differences. Do not change the version range just to unblock dependency resolution.

Vize 0.291.0 requires Rust 1.95 or newer, and its Oxc dependency requires Rust 1.93 or newer. The original Rust 1.85 pin therefore failed before Vue Vet compiled. Keep the exact Rust toolchain aligned with the workspace `rust-version`, regenerate `Cargo.lock` only with that toolchain, and validate upgrades through `just roll-rust`.

## SFC offsets are not plain string positions

Vize block locations are offsets into the original SFC, while downstream parsers may operate on extracted script or template content. Every extraction needs an explicit offset map back to the original source. Unicode makes byte/character confusion visible; CRLF makes line calculations visible.

The template parser reports offsets relative to the extracted template content.
The Vize adapter must add the SFC template block's start offset exactly once
before creating Vue Vet spans. Persist full diagnostic snapshots for ASCII,
Unicode, multiline, and multiple-directive inputs whenever this adapter changes.

## A parsed SFC contains several language surfaces

Template, ordinary script, script setup, styles, and custom blocks have different grammars and semantics. Never run one generic pattern language over the full `.vue` file and present the result as semantic certainty.

Oxc spans are relative to the extracted script block. Add the Vize
`SfcScriptBlock.loc.start` offset exactly once before deriving line and column.
Ordinary script and script setup remain separate fact blocks so duplicate names
and future merge semantics are explicit rather than accidental.

## Configuration is part of diagnostic identity

Preset expansion happens before explicit rule overrides. Path globs normalize
separators before matching, severity overrides run before exit-policy scoring,
and suppressions run after diagnostics exist so unused directives can be
reported. Future cache keys must include the serialized effective configuration.

## ast-grep findings can overlap semantic findings

Custom patterns may rediscover a problem already proven by a built-in rule. Built-in semantic evidence is authoritative. Deduplication and precedence must be deterministic before ast-grep rules ship.

## Diff mode is a graph problem

A changed file can introduce a diagnostic whose best source location is in another file. Changed-line filtering must track causality through the project graph rather than dropping every finding outside the textual diff.

Project resolution is deliberately smaller than Node, TypeScript, Vite, and
Nuxt resolution. Missing relative or supported alias targets are diagnostics;
package imports are external graph nodes; unsupported `#` aliases remain
visible. Never silently reinterpret an unresolved edge as an external package.

Cache corruption is a miss, not a scan failure. Cache keys must change when any
normalized semantic input changes; raw dependency ASTs must never be persisted.
Diff filtering keeps every project diagnostic because removing an edge can
create a finding on a file that has no remaining path back to the changed file.

## The current score is provisional

The initial score is a deterministic vertical-slice implementation, not yet a stable measure of repository health. Before external release, scoring needs documented weights, stable diagnostic fingerprints, baselines, and reference-corpus evidence. Do not optimize rules around the current arithmetic.

## Vue behavior must be capability-gated

Vue compiler behavior is not uniform across Vue 3 releases. Reactive props
destructuring and `useTemplateRef()` are available in Vue 3.5+, while direct
`defineProps()` destructuring loses reactivity in 3.4 and older. Version-aware
rules read the nearest numeric `vue` requirement from `package.json`, include
that manifest in cache inputs, and stay quiet when the capability is unknown.

`watchEffect()` subscribes only to reactive reads reached during synchronous
execution. Model guarded reads as graph edges derived from Oxc control structure;
do not rediscover them with source text matching. A conditional edge is evidence
about tracking behavior, so diagnostics must describe the condition and recommend
explicit `watch` sources when all inputs are intended to invalidate the effect.
