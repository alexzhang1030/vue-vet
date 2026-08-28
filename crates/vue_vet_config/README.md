# vue_vet_config

**`vue-vet.toml`**, path filters, and in-source suppressions for
[Vue Vet](https://github.com/alexzhang1030/vue-vet).

Owns parse/apply of the config file and comment suppressions. Does **not**
discover workspaces, register rules, or analyze — rule-ID validation takes a
caller-supplied `known_rules` set from the session registry.

## Status

Workspace-internal (`publish = false`). Consumed by `vue_vet_session` (and
CLI `--print-config`).

## Contract

| Constant / type | Detail |
| --- | --- |
| `CONFIG_FILE` | `"vue-vet.toml"` |
| `CONFIG_VERSION` | `1` — unknown version fails before scan |
| `Preset` | `recommended` (default) / `none` |
| `RuleLevel` | `off` / `info` / `warning` / `error` |
| `PracticeMode` | `on` (default) / `off` |
| `Config` | `version`, `preset`, `practice`, `include`, `exclude`, `rules` |

Strict hand-rolled TOML subset (not the full `toml` crate): unknown keys and
non-`[rules]` sections are rejected. Default include is `**/*.vue`.

### Apply order

1. `Config::parse` / discover
2. `validate_rules(known_rules)`
3. Analysis produces diagnostics
4. `Config::apply` — preset, practice channel, severity overrides
5. `apply_suppressions` — comment directives; unused disable →
   `vue-vet/config/unused-suppression`

`practice = "off"` drops `PRACTICE_CATEGORY` before scoring/reporting.
`preset = "none"` drops findings without an explicit `[rules]` override.

### Suppressions

Recognized in `<!-- … -->`, `//`, and `/* … */` comments:

- `vue-vet-disable-next-line [rule-id…]`
- `vue-vet-disable [rule-id…]`
- `vue-vet-enable [rule-id…]`

`#` line comments in `vue-vet.toml` respect quoted strings.

## Public API

| Item | Role |
| --- | --- |
| `Config::{parse, validate_rules, apply, path_filter}` | Load + apply |
| `PathFilter::matches` | Include / exclude globs |
| `apply_suppressions` | Post-normalize filter + unused reports |
| `ConfigError` | Parse / validation failures |

## Related docs

- [Root README](../../README.md) (`Configuration`)
- [conventions PCR](../../.agents/docs/conventions.md)
- [Workspace crates](../../docs/crates.md)

## License

MIT
