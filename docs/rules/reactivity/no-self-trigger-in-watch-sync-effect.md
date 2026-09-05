# `vue-vet/reactivity/no-self-trigger-in-watch-sync-effect`

Category: reactivity  
Default severity: warning  
Confidence: high

**Retired.** This ID stays registered for config compatibility and never reports.

Vue 3.5.40 coalesces a synchronous self-assign inside `watchSyncEffect` into **one** initial run (`count.value = count.value + 1`, `count.value++`, or the same write via a helper). An external change of `count` yields a second run. That is not an infinite loop. Runtime evidence: `just oracle-self-trigger` (Node-only run counts on the locked Vue 3.5.40 oracle package; not the onTrack JSON oracle).

Quiet regression (must not report):

```vue
<script setup lang="ts">
import { ref, watchSyncEffect } from 'vue'
const count = ref(0)
watchSyncEffect(() => { count.value = count.value + 1 })
</script>
<template>{{ count }}</template>
```

## Fixtures

- `fixtures/rules/no-self-trigger-in-watch-sync-effect/valid/self-assign.vue`
- `fixtures/rules/no-self-trigger-in-watch-sync-effect/valid/safe.vue`
