# vue_vet_practice

**Ecosystem practice suggestions** for
[Vue Vet](https://github.com/alexzhang1030/vue-vet).

Recipe metadata plus thin `Rule` implementations that consume existing Vue Vet
facts — not a parallel pattern engine. Findings use `category: practice`
(`PRACTICE_CATEGORY`), may attach a `Recommendation`, default to `Info`, and
stay **off** the density score and default CI exit path.

Disable the whole channel with `practice = "off"` in `vue-vet.toml`. Individual
practice rule IDs remain toggleable under `[rules]` when the channel is on.

## Status

Workspace-internal (`publish = false`). Registered beside builtins by
`vue_vet_session`. Current pack: **13** rules (asserted in-crate).

## Public API

| Item | Role |
| --- | --- |
| `practice_rules()` / `practice_registry()` | Rule list / registry |
| `PracticeRecipe` / `EcosystemApi` | Recipe metadata (`meets_vue`, …) |

## Layout

| Module | Role |
| --- | --- |
| `recipe.rs` | `PracticeRecipe` / `EcosystemApi` |
| `rules/` | One module per suggestion id |
| `util.rs` | Shared helpers (`is_setup_lifecycle_hook`, `callee_is`, …) |

### Current rules

| Id segment | Notes |
| --- | --- |
| `prefer-define-model` | `defineProps` + `defineEmits` → `defineModel` (Vue ≥ 3.4) |
| `prefer-to-value` | Prefer `toValue` over manual unref patterns |
| `prefer-use-slots-attrs` | Prefer `useSlots` / `useAttrs` |
| `prefer-use-template-ref` | Historical id under `vue-vet/reactivity/…` for config stability |
| `vueuse-use-*` | VueUse recipes (event listener, observers, timers, window size, …) |

VueUse recipes may adjust help text when `@vueuse/core` is already in the
session `PackageIndex`. Prefer high-precision fact links (shared timer
bindings, lifecycle + missing cleanup, resolved `unref`) over broad call
presence.

## Related docs

- [Rule catalog](../../docs/rules/README.md) (`practice` tier)
- [JSON output](../../docs/json-output.md) (`recommendation`)
- [architecture PCR](../../.agents/docs/architecture.md) / [goal PCR](../../.agents/docs/goal.md)
- [Workspace crates](../../docs/crates.md)

## License

MIT
