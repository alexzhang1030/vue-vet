# `vue-vet/reactivity/no-returned-watcher-cleanup`

Category: reactivity  
Default severity: warning  
Confidence: high  
Tier: tracer

Vue 3.5 watcher APIs (`watch`, `watchEffect`, `watchPostEffect`, `watchSyncEffect`)
do **not** register a function returned from the watcher callback as cleanup.
Only `onCleanup` (the callback argument) and `onWatcherCleanup` attach teardown.
One rule id covers every watcher API: the ignored-return contract is the same
premise.

## Bad

```vue
<script setup lang="ts">
import { ref, watch, watchEffect } from 'vue'

const source = ref(0)
watchEffect(() => {
  source.value
  return () => {}
})
watch(source, () => () => {})
</script>
```

## Good

```vue
<script setup lang="ts">
import { onWatcherCleanup, ref, watchEffect } from 'vue'

const source = ref(0)
watchEffect((onCleanup) => {
  source.value
  const cleanup = () => {}
  onCleanup(cleanup)
})
watchEffect(() => {
  source.value
  onWatcherCleanup(() => {})
})
</script>
```

## Detection

Oxc-backed lifetime facts: the watch/watchEffect-family callback (second
argument for `watch`, first for the effect family) directly returns an arrow or
function, or a same-scope identifier proven to be a function. Alias imports
count; shadowed locals, alias cycles, reassigned function declarations, and
generator callbacks stay quiet. Returning a function that is also registered
with the callback `onCleanup` parameter (including after `await`) or with
synchronous `onWatcherCleanup` stays quiet. Aliases of the same function
identity are equivalent. Watch's cleanup parameter remains slot 2 even when
earlier parameters are destructured. `onCleanup(...[dispose])` and an explicit
`onWatcherCleanup(dispose, false, owner)` captured before `await` stay quiet.
Unknown registrar spreads abstain rather than claiming a missing registration. Watch calls that contain spread arguments stay quiet because the
callback slot is no longer certain. Watch source getters are not callbacks.
Nested returns inside unrelated inner functions stay quiet.

## Applicability

Plain TypeScript modules and Vue SFCs. Async callbacks that return a function
are the same ignored-return behavior.

## Remediation

Call `onCleanup` or `onWatcherCleanup` with the teardown function. Do not rely
on the callback return value.
