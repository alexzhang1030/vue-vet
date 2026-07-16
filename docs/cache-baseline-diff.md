# Cache, baselines, and diff analysis

Vue Vet caches only normalized `ScanSummary` and `ProjectGraph` values. It never
persists Vize or Oxc AST objects. Cache format version 2 uses a SHA-256 key over:

- cache, graph-convention, and built-in ruleset versions;
- Vue Vet, Vize, and Oxc versions;
- the serialized effective configuration;
- every discovered Vue, JavaScript, TypeScript, JSX, and TSX path and byte body.

Writes use a temporary file followed by an atomic rename. Invalid JSON and
unsupported cache versions are deleted and rebuilt without failing the scan.
Version 2 adds rule confidence and documentation metadata to cached diagnostics;
version 1 entries are left untouched and naturally missed under the versioned
cache directory.

Use `--no-cache`, `--cache-dir <dir>`, and `--cache-stats` to control or inspect
the local cache.

## Baselines

`--write-baseline <file>` writes format version 1 with SHA-256 fingerprints.
Fingerprints include the rule ID, normalized path, source offset, and message.
`--baseline <file>` hides only exact matches; moved, changed, or new findings
remain visible.

## Diff mode

`--diff <ref>` invokes Git with argument arrays and reads changed paths as
NUL-delimited data. Per-file findings are retained on added lines. Project-level
findings are always retained because a changed dependency can cause the best
diagnostic location to be in a distant consumer or newly unused component.

This intentionally favors completeness over an aggressively narrow diff. The
graph invalidation inputs are available for future consumer-level scheduling;
incremental results must remain equivalent to a clean scan.

## Initial measurement

On 2026-07-16, the eight-file Nuxt graph fixture took 0.007 seconds for a cold
scan and 0.004 seconds for an immediate cache hit on x86_64 Linux with a warm
binary. This is implementation evidence, not the medium-repository release
budget; future comparisons must preserve the fixture and command line.
