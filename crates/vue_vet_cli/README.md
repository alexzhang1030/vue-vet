# vue-vet (CLI)

Binary package for [Vue Vet](https://github.com/alexzhang1030/vue-vet).

- Directory: `crates/vue_vet_cli`
- Cargo package / binary name: `vue-vet`
- End-user install: [`@vue-vet/cli`](https://www.npmjs.com/package/@vue-vet/cli)
  or GitHub Release archives — not crates.io (`publish = false`)

Thin clap front-end over `vue_vet_session`: flags, stdout/stderr policy, exit
codes, private fix preview/apply, reactivity TUI, and `--lsp` / `--mcp` handoff
to `vue_vet_lsp` / `vue_vet_mcp`.

**Do not grow analysis logic here** — session / project / rules own that.

## Layout

| Module | Role |
| --- | --- |
| `main.rs` | Clap + scan / format / exit dispatch |
| `report.rs` | Digests, summaries, operational errors |
| `explain.rs` | `--explain` / `--explain-scope` |
| `fixes.rs` | `--fix-dry-run` / `--fix-safe` (atomic apply) |
| `reactivity_tui.rs` | Interactive binding navigator |
| `tests/cli` | Explain / fix / report / cache / project |

## Notable flags

| Flag | Behavior |
| --- | --- |
| `--format text\|json\|sarif\|github` | Reporter selection |
| `--color` / `--progress` | `auto` / `always` / `never` (progress: stderr only) |
| `--config` / `--print-config` | Config path / effective JSON |
| `--lsp` / `--mcp` | Adapter entry (exclusive modes) |
| `--explain` / `--explain-scope` | Docs / “would Vue re-run?” |
| `--print-graph` / `--print-reactivity` / `--reactivity-tui` | Graph / tracer surfaces |
| `--no-cache` / `--cache-dir` / `--cache-stats` | Cache control |
| `--baseline` / `--write-baseline` / `--diff` | Presentation filters |
| `--threads` | Shared Rayon pool size |
| `--fix-dry-run` / `--fix-safe` | Safe edits (force fresh scan) |
| `--deny-warnings` | Exit 1 on warnings too |

Exit codes: `0` pass, `1` diagnostics threshold, `2` operational failure.

## Constraints

- Progress streams on **stderr** only (`auto` = TTY stderr and not `CI`); never
  pollute JSON stdout.
- Fix modes fail closed on multi-file plans; see [edit model](../../docs/edit-model.md).
- Baseline / diff filter after cache lookup so they do not fragment cache keys.

## Related docs

- [Root README](../../README.md)
- [Install](../../docs/install.md)
- [JSON output](../../docs/json-output.md)
- [Edit model](../../docs/edit-model.md)
- [architecture PCR](../../.agents/docs/architecture.md)
- [Workspace crates](../../docs/crates.md)

## License

MIT
