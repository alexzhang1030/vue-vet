# vue_vet_rule_query

Shared **fact queries** for Vue Vet built-in and practice rules.

Rules consume Vue Vet-owned facts only. This crate does not depend on Vize or
Oxc types. It is a workspace-internal query layer — not a new rule pack and not
part of the published `vue_vet_core` contract.

## Status

Workspace-internal (`publish = false`). Used by `vue_vet_rules` and
`vue_vet_practice` for setup-block walks, after-await call selection, graph
lookups, and prior-unconditional-read helpers.

## Modules

| Module | Role |
| --- | --- |
| `blocks` | Setup / ordinary script block access, after-await call slices |
| `graph` | Reactive / script binding lookups, template ref names |
| `reads` | Member paths, guards, unconditional vs conditional reads |

Add a sibling helper here when two or more rules need the same fact walk. Do not
grow individual rule modules with duplicated control-flow queries.

## Related docs

- [architecture PCR](../../.agents/docs/architecture.md) (`vue_vet_rule_query`)
- [conventions PCR](../../.agents/docs/conventions.md)
- [Workspace crates](../../docs/crates.md)

## License

MIT
