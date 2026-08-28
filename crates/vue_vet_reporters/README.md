# vue_vet_reporters

Deterministic **text and machine-readable reporters** for
[Vue Vet](https://github.com/alexzhang1030/vue-vet).

Consumes Vue Vet-owned `ScanSummary` / explain models. Does not own session
state, run analysis, or mutate files. Formats: human text, JSON schema v1,
SARIF 2.1.0, GitHub Actions annotations, plus explain / scope / reactivity /
binding_nav / component_nav helpers.

## Status

Workspace-internal (`publish = false`). Shared by CLI, MCP scan JSON, and
explain surfaces so diagnostic identity stays consistent.

## Layout

```text
lib.rs            ReportContext + format dispatch
json.rs / text.rs / sarif.rs / github.rs
explain.rs        rule / finding / scope documentation render
reactivity.rs     digest + modules_detail
binding_nav.rs / component_nav.rs
```

## Related docs

- [JSON output](../../docs/json-output.md)
- [architecture PCR](../../.agents/docs/architecture.md) (reporting)
- [Workspace crates](../../docs/crates.md)

## License

MIT
