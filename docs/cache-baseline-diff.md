# Cache, baselines, and diff analysis

Vue Vet caches only normalized `ScanSummary` and `ProjectGraph` values. It never
persists Vize or Oxc AST objects. The content key is a SHA-256 over:

- cache, graph-convention, ruleset, and reactivity-graph versions;
- Vue Vet tool version plus Vize / Oxc / oxc_resolver identity fields;
- the serialized effective configuration;
- every discovered Vue, JavaScript, TypeScript, JSX, and TSX path and byte body.

See `vue_vet_cache::CACHE_FORMAT_VERSION` / `content_key` for the authoritative
field list ([crate README](../crates/vue_vet_cache/README.md)).

Discovery produces one immutable `WorkspaceInputSnapshot`. Cache hashing and a
cache-miss analysis consume the same bytes, so a miss never performs a second
walk/read pass. `package.json`, lockfiles, and resolver configs remain
invalidation inputs, but report coverage lists only source files that were
actually analyzed.

Writes use a temporary file followed by an atomic rename. Invalid JSON and
unsupported cache versions are deleted and rebuilt without failing the scan.
`CACHE_FORMAT_VERSION` bumps whenever the on-disk schema changes; older
entries are left untouched and naturally missed under the versioned cache
directory. Fix modes bypass cache regardless, so mutation always starts from a
fresh scan.

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

The long-lived session retains per-file facts and a reverse-dependency index.
`apply_changes` / `analyze_affected` reparses changed files, reuses unaffected
file-rule results, and invalidates dependent consumers; normal edits neither
walk/read the whole workspace nor rebuild unrelated project partitions, and the
merged graph remains deterministic and equivalent to a clean scan
([architecture](../.agents/docs/architecture.md#performance-model-oxlint-inspired)).
