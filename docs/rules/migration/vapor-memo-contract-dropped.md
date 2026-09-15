# Vapor memo contract dropped

Default severity: **info**. Category: **migration** (excluded from score and default CI exit). Group: **vapor-migration**. Opt-in.

One finding per `v-memo` directive. `v-once` does not trigger this rule.

## Audited toolchain tuple

Same as [`vapor-assessment`](./vapor-assessment.md): Vue `3.6.0-rc.7` and `@vitejs/plugin-vue` `6.0.8`.

## Verdict table

| Surface | Verdict |
| --- | --- |
| Any `v-memo` | `blocked` (reason includes the memo expression) |
| No `v-memo` | `not-applicable` |

## Why

Vapor drops the `v-memo` contract. A file that still uses it cannot be a compiler candidate.

## Safe patterns

Remove `v-memo`. `v-once` is a different directive and stays quiet here.
