# Known gotchas

## Vize API churn is expected

Vize is not yet production-stable and publishes frequently. Keep the dependency exact-pinned. An upgrade is a compatibility task: compile, inspect API changes, run golden fixtures and diagnostic snapshots, and record behavior differences. Do not change the version range just to unblock dependency resolution.

## SFC offsets are not plain string positions

Vize block locations are offsets into the original SFC, while downstream parsers may operate on extracted script or template content. Every extraction needs an explicit offset map back to the original source. Unicode makes byte/character confusion visible; CRLF makes line calculations visible.

## A parsed SFC contains several language surfaces

Template, ordinary script, script setup, styles, and custom blocks have different grammars and semantics. Never run one generic pattern language over the full `.vue` file and present the result as semantic certainty.

## ast-grep findings can overlap semantic findings

Custom patterns may rediscover a problem already proven by a built-in rule. Built-in semantic evidence is authoritative. Deduplication and precedence must be deterministic before ast-grep rules ship.

## Diff mode is a graph problem

A changed file can introduce a diagnostic whose best source location is in another file. Changed-line filtering must track causality through the project graph rather than dropping every finding outside the textual diff.

## The current score is provisional

The initial score is a deterministic vertical-slice implementation, not yet a stable measure of repository health. Before external release, scoring needs documented weights, stable diagnostic fingerprints, baselines, and reference-corpus evidence. Do not optimize rules around the current arithmetic.

