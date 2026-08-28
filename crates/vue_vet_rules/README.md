# vue_vet_rules

Built-in **semantic lint / gate rules** for
[Vue Vet](https://github.com/alexzhang1030/vue-vet).

Every rule is a self-contained module under `src/rules` with stable `RuleMeta`,
fixtures, docs, and exact-span snapshots. The parent module only assembles
`builtin_registry()`. Rules consume Vue Vet facts (often via
`vue_vet_rule_query`) — never Vize or Oxc AST. Practice suggestions live in
`vue_vet_practice`, not here.

## Status

Workspace-internal (`publish = false`). Recommended preset is high-confidence
only. Matrix families (tracking-graph / after-await packs) share a detection
type plus a unique-id catalog under `rules/matrix/`.

## Related docs

- [Rule catalog](../../docs/rules/README.md)
- [conventions PCR](../../.agents/docs/conventions.md)
- [vue_vet_rule_query](../vue_vet_rule_query/README.md)
- [Workspace crates](../../docs/crates.md)

## License

MIT
