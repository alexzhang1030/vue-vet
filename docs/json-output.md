# JSON output contract

Vue Vet emits machine-readable results with `--format json`. The current wire
format is version 1:

```json
{
  "schema_version": 1,
  "tool": { "name": "vue-vet", "version": "0.1.0" },
  "ok": true,
  "mode": "full",
  "project": {
    "root": ".",
    "framework": "vue",
    "analyzed_files": ["src/App.vue"],
    "analyzed_file_count": 1,
    "files_scanned": 1,
    "complete": true,
    "skipped_checks": [],
    "skipped_check_reasons": {}
  },
  "diagnostics": [],
  "summary": {
    "score": 100,
    "finding_count": 0,
    "affected_file_count": 0,
    "by_severity": { "info": 0, "warning": 0, "error": 0 }
  },
  "error": null
}
```

## Diagnostic identity

Each diagnostic includes an opaque `id` with a readable prefix:

```text
<normalized-file>::<line>:<column>::<rule-id>::<content-digest>
```

The identity is deterministic for an unchanged finding. It changes when its
normalized location, rule, severity, or user-visible message changes. Consumers
must compare it as an opaque string rather than parsing or constructing it.

Diagnostic `file` values and `project.analyzed_files` are relative to
`project.root` and use `/` separators on every operating system. `confidence` and `documentation`
come from Vue Vet-owned rule metadata. `documentation` is a repository-local
Markdown path so local tools and coding agents can read the exact rule guidance
without a network request.

## Completeness

An empty `diagnostics` array is clean only when `project.complete` is `true`.
Consumers must inspect `skipped_checks` and `skipped_check_reasons` when it is
false. `analyzed_files` is sorted and deduplicated so CI and agent consumers can
verify exact coverage instead of inferring it from a count.

`mode` is one of `full`, `baseline`, or `diff`. Filtering changes the reported
findings, not the analyzed-file coverage.

With `--format json`, operational failures also use version 1 and retain exit
code 2. They set `ok` and `project.complete` to `false`, leave diagnostics and
coverage empty when the scan never completed, set `summary.score` to `null`, and
provide the actionable failure in `error.message`. Text mode continues to write
operational failures to stderr.

## Agent consumption

The JSON report is the complete fact layer, not a generated fix prompt. Agents
should group diagnostics by `rule_id`, prioritize severity and confidence, read
the referenced source and local documentation, and verify a finding before
editing. Future bounded handoff prompts may point to this report, but must not
replace it or silently omit lower-priority findings.

## Compatibility

- `schema_version` is required; consumers must branch on versions they support.
- Version 1 is the initial public report contract and includes explicit tool,
  mode, coverage, identity, metadata, summary, and error fields.
- Before the first alpha release, version 1 may change without compatibility
  guarantees. After alpha, incompatible changes require a new schema version.
- Adding optional fields is backward-compatible within version 1.
- Removing fields, changing their meaning or type, or changing diagnostic
  identity semantics requires a new schema version.
- Object field order is not significant. Array order is deterministic.

Configuration, graph debug output, cache files, and baselines remain
independently versioned formats.
