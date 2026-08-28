# vue_vet_practice

**Ecosystem practice suggestions** for
[Vue Vet](https://github.com/alexzhang1030/vue-vet).

Recipe metadata plus thin `Rule` implementations that consume existing Vue Vet
facts (no parallel pattern engine). Findings use `category: practice`, may
attach a `recommendation` payload, and stay off the score / default CI exit
path. Disable the whole channel with `practice = "off"` in `vue-vet.toml`.

## Status

Workspace-internal (`publish = false`). Registered beside builtins by
`vue_vet_session`.

## Layout

```text
recipe.rs   PracticeRecipe / EcosystemApi metadata
rules/      one module per suggestion id
util.rs     shared helpers (lifecycle hooks, callee checks)
```

## Related docs

- [Rule catalog](../../docs/rules/README.md) (`practice` tier)
- [architecture PCR](../../.agents/docs/architecture.md) (`vue_vet_practice`)
- [goal PCR](../../.agents/docs/goal.md)
- [Workspace crates](../../docs/crates.md)

## License

MIT
