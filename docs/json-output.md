# JSON output contract

Vue Vet emits machine-readable results with `--format json`.

```json
{
  "schema_version": 1,
  "files_scanned": 1,
  "diagnostics": [],
  "score": 100
}
```

## Compatibility

- `schema_version` is required and identifies the top-level report contract.
- Adding optional fields is backward-compatible within version 1.
- Removing fields, changing their meaning or type, or changing diagnostic
  identity semantics requires a new schema version.
- Object field order is not significant.
- Consumers must reject versions they do not support.
- Paths use the same normalized logical paths as text output and diagnostic
  fingerprints.

The Alpha contract covers `files_scanned`, `diagnostics`, `score`, and the
nested diagnostic and source-span fields. Configuration, graph debug output,
cache files, and baselines are independently versioned formats.
