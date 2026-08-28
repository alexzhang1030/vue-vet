# vue-vet (CLI)

Binary package for [Vue Vet](https://github.com/alexzhang1030/vue-vet). Directory
`crates/vue_vet_cli`; Cargo package / binary name `vue-vet`.

Thin clap front-end over `vue_vet_session`: discovery, scan, report, explain,
safe fixes, cache/baseline/diff flags, reactivity TUI, plus `--lsp` / `--mcp`
entry points that hand off to `vue_vet_lsp` / `vue_vet_mcp`. No analysis logic
belongs here beyond flag wiring and stdout/stderr policy.

## Status

Workspace-internal (`publish = false`). End users install via
[`@vue-vet/cli`](https://www.npmjs.com/package/@vue-vet/cli) or GitHub Release
archives — see [install docs](../../docs/install.md).

## Layout

```text
main.rs           clap + scan / format / exit dispatch
report.rs         digests, summaries, operational errors
explain.rs        --explain / --explain-scope
fixes.rs          --fix-dry-run / --fix-safe
reactivity_tui.rs interactive binding navigator
tests/cli         explain / fix / report / cache / project
```

## Related docs

- [Root README](../../README.md) — user-facing flags and config
- [JSON output](../../docs/json-output.md)
- [architecture PCR](../../.agents/docs/architecture.md)
- [Workspace crates](../../docs/crates.md)

## License

MIT
