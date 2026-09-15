# Vapor migration assessment

Default severity: **info**. Category: **migration** (excluded from score and default CI exit). Group: **vapor-migration**. Opt-in.

One finding per assessed `.vue` file when the group is enabled, including when every check is fine. Quiet output is still incomplete when `complete` is false.

The diagnostic carries an additive `assessment` object (`kind: "vapor-migration"`).

Two explicit answers:

1. **`convertible`** (`yes` / `no` / `unknown`) — can this component be converted to Vapor at all? `no` when any check is `blocked` or `unsupported`. `unknown` when not `no` and `complete` is false. `yes` otherwise.
2. **`aggregate`** — is direct conversion recommended right now? `ready` only when `complete` is true, no check is `blocked` / `unsupported` / `needs-verification`, and `runtime-envelope` is `compiler-candidate`.

## Audited toolchain tuple

| Package | Version |
| --- | --- |
| `vue` / `@vue/compiler-sfc` / `@vue/compiler-vapor` / `@vue/runtime-vapor` | `3.6.0-rc.7` (vuejs/core `4b2f1914e8a6da7218955593b8bc2ba5db2c6dce`) |
| `@vitejs/plugin-vue` | `6.0.8` (`d8ff7d0e8f557a7c1975c07b30e232c69bdbbc03`) |

## Verdict table

| Outcome | Meaning |
| --- | --- |
| `ready` | Direct conversion recommended: complete, inside the verified runtime envelope, no blocking or open checks. |
| `compiler-candidate` | Static facts support a Vapor compile path, but the aggregate was not promoted to `ready`. |
| `blocked` | A construct is proven unsupported under the audited default compiler. |
| `needs-verification` | App, plugin, runtime, dependency, or hydration behavior is still open. |
| `unsupported` | Resolved toolchain identity is outside the audited tuple (and not the 3.5 compiler-sfc gap). |
| `not-applicable` | Check does not apply; never wins the aggregate. |

Aggregate of checks is the worst of `blocked` > `unsupported` > `needs-verification` > `compiler-candidate`. If `complete` is false, that worst cannot be better than `needs-verification`. `ready` is the extra promotion above.

## Why

Vue Vapor compilation is a closed capability envelope. This rule records that envelope per SFC without changing the correctness score.

## Safe patterns

Enable the group only when you want the assessment. Default configuration emits nothing from `migration`.

## Enable

- `--group vapor-migration`
- `assessment = "vapor"` in `vue-vet.toml`
- an individual ID under `[rules]`
