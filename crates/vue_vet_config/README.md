# vue_vet_config

**Configuration**, presets, path filters, and suppressions for
[Vue Vet](https://github.com/alexzhang1030/vue-vet).

Discovers and parses `vue-vet.toml` (`CONFIG_VERSION = 1`): preset
(`recommended` / `none`), per-rule levels, `practice` channel on/off, include /
exclude globs, and scoped suppressions. Applies suppressions after diagnostic
normalization and can emit unused-suppression findings.

## Status

Workspace-internal (`publish = false`). Loaded by `vue_vet_session`; rule IDs
are validated against the session registry.

## Surface

| Item | Role |
| --- | --- |
| `Config` / `CONFIG_FILE` | Parsed `vue-vet.toml` |
| `Preset` / `RuleLevel` / `PracticeMode` | Effective enablement |
| `PathFilter` | Include / exclude matching |
| `apply_suppressions` | Post-normalize filter + unused reports |

## Related docs

- [Root README](../../README.md) (`Configuration`)
- [conventions PCR](../../.agents/docs/conventions.md)
- [Workspace crates](../../docs/crates.md)

## License

MIT
