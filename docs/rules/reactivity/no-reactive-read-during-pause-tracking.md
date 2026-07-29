# `vue-vet/reactivity/no-reactive-read-during-pause-tracking`

Category: reactivity  
Default severity: warning  
Confidence: high  
Tier: tracer

Reads between `pauseTracking()` and `enableTracking()` / `resetTracking()` are
outside synchronous dependency collection. The effect will not re-run when those
values change.

## Bad

```vue
<script setup lang="ts">
import { enableTracking, pauseTracking, ref, watchEffect } from 'vue'

const value = ref(0)
watchEffect(() => {
  pauseTracking()
  console.log(value.value)
  enableTracking()
})
</script>
```

## Good

```vue
<script setup lang="ts">
import { enableTracking, pauseTracking, ref, watchEffect } from 'vue'

const value = ref(0)
watchEffect(() => {
  const current = value.value
  pauseTracking()
  enableTracking()
  console.log(current)
})
</script>
```

## Detection

Fact-driven: tracking-scope reads classified `OutsideTracking` when the script
calls `pauseTracking`.

## Remediation

Read dependencies before pausing tracking, or list them in explicit `watch` sources.
