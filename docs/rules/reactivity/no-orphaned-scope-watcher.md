# `vue-vet/reactivity/no-orphaned-scope-watcher`

Category: reactivity  
Default severity: warning  
Confidence: high  
Tier: tracer

`effectScope().run` captures watchers created **synchronously** while the
callback is the active scope. An ignored `watch` / `watchEffect` handle created
after `await` inside a proven `run(async () => …)` is not stopped by
`scope.stop()`. One rule id covers watcher API variants.

## Bad

```vue
<script setup lang="ts">
import { effectScope, ref, watchEffect } from 'vue'

const source = ref(0)
const scope = effectScope()
await scope.run(async () => {
  await Promise.resolve()
  watchEffect(() => {
    source.value
  })
})
scope.stop()
</script>
```

## Good

```vue
<script setup lang="ts">
import { effectScope, ref, watchEffect } from 'vue'

const source = ref(0)
const scope = effectScope()
scope.run(() => {
  watchEffect(() => {
    source.value
  })
})
scope.stop()
</script>
```

## Detection

Proven `effectScope` binding or `effectScope().run(...)`, async `run` callback
(inline or a named function registered as that `run` argument), straight-line
`await`, then a watcher call whose stop handle is unused (expression statement
or `void`). Assigned, returned, or passed handles stay quiet. Synchronous
creation and explicit synchronous `scope.run` re-entry stay quiet.

`scope.on()` / `scope.off()` inside the same `run` callback stays quiet.
Runtime (Vue 3.5.40) shows `scope.on()` after `await` re-attaches the active
scope so `scope.stop()` owns the watcher (`runsAfterStop=1`). Passing the
scope to an unknown helper (`helper.run(scope)`), assigning `scope.run` or
`scope['run']`, aliasing the scope value, deleting `run`, or exporting the
scope binding (`export const scope`, `export { scope }`, `export default scope`)
makes the owner unproven. Named `run` callbacks keep independently proven
owners when a sibling registration escapes. Unexported local scopes stay
reportable.

## Applicability

Plain TypeScript and SFC.

## Remediation

Create the watcher before `await`, re-enter `scope.run` synchronously, or retain
the stop handle and stop it yourself.
