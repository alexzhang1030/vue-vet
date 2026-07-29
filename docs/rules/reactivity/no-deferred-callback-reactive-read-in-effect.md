# `vue-vet/reactivity/no-deferred-callback-reactive-read-in-effect`

Category: reactivity  
Default severity: warning  
Confidence: high  
Tier: tracer

`watchEffect` (and flush variants) only track reactive reads during synchronous
execution. Reads inside `nextTick` / `Promise.then` (and similar deferred
callbacks) do not subscribe the effect.

## Bad

```vue
<script setup lang="ts">
import { nextTick, ref, watchEffect } from 'vue'

const value = ref(0)
watchEffect(() => {
  nextTick(() => {
    console.log(value.value)
  })
})
</script>
```

## Good

```vue
<script setup lang="ts">
import { nextTick, ref, watchEffect } from 'vue'

const value = ref(0)
watchEffect(() => {
  const current = value.value
  nextTick(() => {
    console.log(current)
  })
})
</script>
```

## Detection

Fact-driven: effect-family scopes with `OutsideTracking` reads when the file does
not use `pauseTracking` (pause windows are covered by
`no-reactive-read-during-pause-tracking`). After-`await` reads stay on
`no-after-await-watch-effect-dependency`.

## Remediation

Read dependencies synchronously in the effect body, or use explicit `watch` sources.
