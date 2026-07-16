# Edit model

Vue Vet represents proposed source changes independently of diagnostics,
reporters, parser ASTs, and file application. The first M3 slice is preview-only:
it validates and orders edits but does not write files.

## Contract

Each `TextEdit` contains:

- `file`: the repository-relative target path;
- `range.offset` and `range.length`: a byte range in the original file;
- `replacement`: the exact replacement text;
- `applicability`: `safe` or `unsafe`;
- `rule_id`: the rule that proposed the edit.

`safe` means the edit may eventually participate in an explicitly requested
safe-fix workflow. `unsafe` edits always require a separate explicit opt-in.
Neither value currently authorizes mutation because this slice has no apply API.

```json
{
  "file": "src/App.vue",
  "range": { "offset": 42, "length": 6 },
  "replacement": "sanitizedHtml",
  "applicability": "unsafe",
  "rule_id": "vue-vet/security/no-v-html"
}
```

## Planning and conflicts

`EditPlan` checks that every `offset + length` is representable, sorts edits by
normalized path and byte range, and rejects conflicting edits in the same file.
Non-empty ranges are half-open, so adjacent replacements do not overlap.
Insertions at either boundary of a replacement are rejected conservatively
because applying them in a different order can change the output. Multiple
insertions at the same byte offset also conflict.

Later issue #9 slices will add preview output, transactional file replacement,
rollback, encoding and line-ending preservation, and a post-fix rescan. Those
operations must consume a validated plan rather than reimplementing conflict
rules.
