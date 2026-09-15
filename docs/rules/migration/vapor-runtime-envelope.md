# Vapor runtime envelope

Default severity: **info**. Category: **migration** (excluded from score and default CI exit). Group: **vapor-migration**. Opt-in.

One finding per construct that sits outside the admitted runtime envelope. No findings means this check is `compiler-candidate` — “nothing outside the envelope” is the positive statement. This check is never `not-applicable`.

The whitelist lives in `ENVELOPE_VUE_BUILT_IN_DIRECTIVES` / `ENVELOPE_SCRIPT_APIS` in `vapor_migration.rs`. Widening it requires first adding and passing a runtime oracle fixture pair under [`research/vapor-migration`](../../../research/vapor-migration/README.md).

## Admitted envelope

Oracle-executed (`research/vapor-migration` “Admitted envelope”):

| Construct | Status |
| --- | --- |
| `ref` | inside |
| text interpolation | inside |
| `v-if` / `v-else` / `v-else-if` | inside |
| keyed `v-for` on a ref array | inside |
| native event handlers **without** modifiers | inside (click was executed; any modifier-free `v-on` is the only generalization) |
| `v-once` | inside |
| `v-bind` / `:attr` (including object `v-bind`) | inside |
| `<template>` with `v-if` / `v-for` | inside (the wrapper); unkeyed `v-for` still reports |
| `v-memo` | **blocked** by `vapor-memo-contract-dropped`, not this check |

## Outside the envelope (`needs-verification`, exact span)

| Construct | Span |
| --- | --- |
| `v-model` | the directive |
| `v-show` | the directive |
| `v-html` | the directive |
| `v-text` | the directive |
| `v-slot` / `#name` | the directive |
| `<slot>` | the element start tag |
| `v-for` without `:key` | the `v-for` directive |
| event handlers with modifiers (`@click.stop`, `@keyup.enter`, …) | the directive |
| custom directives (name not in `if/else/else-if/for/on/bind/model/slot/pre/once/memo/cloak/show/html/text/is`) | the directive |
| `<component :is>` / `v-is` / `:is` | the element |
| `provide`, `inject`, `defineModel`, `defineSlots`, `useSlots`, `useAttrs`, `defineExpose`, `getCurrentInstance`, `h`, `render` | the `ScriptCallFact` span |

Interop built-ins and child components stay on `vapor-interop-required`. Do not report them here.

## Why

The audited jsdom runtime only executed the admitted envelope. Everything else is untested at this pin.

## Safe patterns

`@click="go"` without modifiers, keyed `v-for`, `v-once`, and object `v-bind` stay quiet.
