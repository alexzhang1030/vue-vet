# vue_vet_rule_query

Shared **fact queries** for Vue Vet built-in and practice rules.

Depends only on `vue_vet_core`. Does not import Vize or Oxc. Not a rule pack and
not part of the published `vue_vet_core` contract — a workspace-internal query
layer so rules do not duplicate setup-block / after-await / member-path walks.

Put a helper here when **two or more** rules need the same predicate. Keep
`vue_vet_core` as the fact/diagnostic contract.

## Status

Workspace-internal (`publish = false`). Dependents: `vue_vet_rules`,
`vue_vet_practice` only.

## Modules

### `blocks` — script-block access

| Item | Role |
| --- | --- |
| `is_setup_block` / `script_block` / `setup_blocks` | Kind-filtered block access |
| `block_calls` / `script_has_call` | Callee-name filters |
| `first_top_level_await_end` | First await end offset in a block |
| `setup_calls_after_first_top_level_await` | After-await call slice (`offset >= first`) |
| `extra_setup_calls` | Second+ setup calls (define\* once-only) |

### `graph` — binding lookups

| Item | Role |
| --- | --- |
| `reactive_binding` / `script_binding` | Name → first fact |
| `script_binding_at` | Name + declaration span → script symbol |
| `static_template_ref_names` / `used_reactive_names` | Template / usage sets |

### `reads` — paths and control-flow

| Item | Role |
| --- | --- |
| `MemberPath` / `member_path` / `binding_path` / `write_path` / `guard_path` | Path identity |
| `same_target` / `join_member_paths` | Path equality / join |
| `has_prior_unconditional_read` / `unguarded_conditional_reads` | Ordering predicates |
| `unconditional_self_triggers` / `effect_family` / `is_readonly_kind` | Scope helpers |

Helpers return borrowed views (`&T` iterators). `SourceSpan` is `Copy` — pass
`call.span` into `report` without cloning.

## Related docs

- [architecture PCR](../../.agents/docs/architecture.md) (`vue_vet_rule_query`)
- [conventions PCR](../../.agents/docs/conventions.md)
- [Workspace crates](../../docs/crates.md)

## License

MIT
