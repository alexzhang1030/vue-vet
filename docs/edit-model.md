# Edit model

Vue Vet represents proposed source changes with Vue Vet-owned types independent
of parser ASTs, reporters, and disk I/O. Active diagnostics carry their edit
candidates at runtime so configuration and suppressions cannot leave an
orphaned fix behind.

## Contract

Each `TextEdit` contains:

- `file`: the repository-relative target path;
- `range.offset` and `range.length`: a byte range in the original file;
- `replacement`: the exact replacement text;
- `applicability`: `safe` or `unsafe`;
- `rule_id`: the rule that proposed the edit.

`safe` means the edit may participate only in an explicitly requested
`--fix-safe` workflow. `unsafe` edits never enter that plan. Applicability alone
does not authorize mutation: the diagnostic must remain active after severity
configuration and scoped suppressions.

```json
{
  "file": "src/App.vue",
  "range": { "offset": 42, "length": 6 },
  "replacement": "",
  "applicability": "safe",
  "rule_id": "vue-vet/accessibility/no-autofocus"
}
```

## Planning and conflicts

`EditPlan` checks that every `offset + length` is representable, sorts edits by
normalized path and byte range, and rejects conflicting edits in the same file.
Non-empty ranges are half-open, so adjacent replacements do not overlap.
Insertions at either boundary of a replacement are rejected conservatively
because applying them in a different order can change the output. Multiple
insertions at the same byte offset also conflict.

The file executor also rejects targets outside the scan scope, ranges outside
the source, ranges that split UTF-8 code points, and any plan spanning more than
one file. Every validation completes before the destination is opened. Valid
edits apply from the end of the original source so earlier byte ranges remain
stable.

## CLI modes

`--fix-dry-run` validates the safe plan without writing. Text output retains the
current diagnostics, while JSON diagnostics include optional `edits` with the
exact normalized path, byte range, replacement, applicability, and rule ID.

`--fix-safe` atomically replaces the one supported target file and then performs
a fresh scan. Its stdout is the post-fix report, including any residual or newly
introduced diagnostics. Both modes bypass the content cache and cannot be
combined with baseline, diff, baseline writing, configuration printing, or
project-graph printing.

The first producer removes boolean `autofocus`, including adjacent horizontal
whitespace. A valued form such as `autofocus="true"` stays diagnostic-only
because the current template fact does not cover the complete attribute.

## Current phase limit

This slice is transactional for one file: content is written to a temporary
sibling and atomically committed only after the whole plan validates. A plan
that reaches two files fails before the first write. Issue #9 remains open for
cross-file staging and rollback, additional safe producers, and explicit unsafe
opt-in; no speculative fix is accepted. This slice guarantees atomic file
contents and preserves untouched UTF-8 bytes, including line endings. A durable
policy for timestamps, ACLs, extended attributes, and other platform metadata
is also still follow-up work.
