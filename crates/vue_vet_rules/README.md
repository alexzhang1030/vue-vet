# vue_vet_rules

Built-in **semantic lint / gate rules** for
[Vue Vet](https://github.com/alexzhang1030/vue-vet).

Every rule is a self-contained module under `src/rules` owning its `RuleMeta`
and `Rule` impl. The parent module only assembles `builtin_registry()`. Rules
consume Vue Vet facts (often via `vue_vet_rule_query`) — never Vize or Oxc AST.

Practice suggestions live in `vue_vet_practice`, not here.

## Status

Workspace-internal (`publish = false`). Recommended preset is
`Confidence::High` only; registry metadata is sorted by stable rule id.

## Public API

| Item | Role |
| --- | --- |
| `builtin_rules()` | `&'static dyn Rule` list |
| `builtin_registry()` | `RuleRegistry` over builtins |

Per-rule docs and fixtures: [`docs/rules/`](../../docs/rules/README.md). After
adding or renaming ids, regenerate the catalog with `just rules-catalog`.

## Layout

| Area | Role |
| --- | --- |
| `rules/*.rs` | Standalone rules (one file per id family) |
| `rules/matrix/` | Tracking-graph / after-await registrar packs: shared detection type + unique `RuleMeta` catalog |
| `rules/directives/` | Directive validity / pairing helpers |
| `rules/tracer_extra.rs` / `graph_extra.rs` | Tracer- and graph-backed extras assembled into the registry |

Pass API: declare `fact_kinds`, implement `run_on` for per-fact checks, use
`run_once` only for true multi-fact aggregation. Prefer immediate `report`
inside the visitor — do not collect the whole fact set into a `Vec` and
re-scan.

## Constraints

- Every built-in change needs metadata, documentation, positive fixtures,
  safe-pattern fixtures, false-positive regressions, exact-span assertions, and
  reporter snapshots ([conventions](../../.agents/docs/conventions.md)).
- Shared block / control-flow walks belong in `vue_vet_rule_query`, not copied
  into each rule module.

## Related docs

- [Rule catalog](../../docs/rules/README.md)
- [vue_vet_rule_query](../vue_vet_rule_query/README.md)
- [conventions PCR](../../.agents/docs/conventions.md)
- [Workspace crates](../../docs/crates.md)

## License

MIT
