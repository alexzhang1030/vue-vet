# `vue-vet/reactivity/no-effect-scope-callback-argument`

Category: reactivity  
Default severity: warning  
Confidence: high

`effectScope(callback)` stores a truthy first argument as the detached option.
The function body stays dormant. Detached ownership must be explicit
(`effectScope(true)`); this rule does not rewrite the call.

This ID is independent of after-await lifetime rules
(`no-orphaned-scope-watcher`, `no-late-scope-dispose`).

## Bad

```vue
<script setup lang="ts">
import { effectScope, ref } from 'vue'
const n = ref(0)
effectScope(() => {
  n.value++
})
</script>
```

## Good

```vue
<script setup lang="ts">
import { effectScope, ref } from 'vue'
const n = ref(0)
const scope = effectScope()
scope.run(() => {
  n.value++
})
const detached = effectScope(true)
detached.run(() => {
  n.value++
})
</script>
```

`effectScope(true)` / `effectScope(false)` and other proven booleans stay
quiet: the first argument is the detached option, not a callback. Spreads,
unknown wrappers, and shadowed imports stay quiet. A closed unchanged function
binding is followed only with exact identity. This rule does not prove
detached ownership beyond a function-shaped first argument.

## Detection

Fact-driven via Vue Vet source-contract facts (Oxc symbol identity + argument
provenance). Proven Vue APIs are named/namespace imports from `vue`,
`vue-demi`, `@vue/runtime-core`, `@vue/runtime-dom`, and `@vue/reactivity`,
plus named `#imports` specifiers that re-export those APIs.

## Remediation

Call `effectScope()` or `effectScope(true)` when detached ownership is
intended, then `scope.run(callback)`.

## Fixtures

- Invalid: `fixtures/rules/no-effect-scope-callback-argument/invalid/`
- Valid: `fixtures/rules/no-effect-scope-callback-argument/valid/`
