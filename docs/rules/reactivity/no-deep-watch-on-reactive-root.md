# `vue-vet/reactivity/no-deep-watch-on-reactive-root`

Category: reactivity  
Default severity: warning  
Confidence: high  
Tier: tracer

`watch(reactiveObject)` deep-tracks the reactive root. Vue Vet records that as a
deep-watch sentinel (`property: "*"`). Prefer a getter or explicit sources so
invalidation stays precise.

## Bad

```vue
<script setup lang="ts">
import { reactive, watch } from 'vue'

const state = reactive({ count: 0 })
watch(state, () => {
  console.log(state.count)
})
</script>
```

## Good

```vue
<script setup lang="ts">
import { reactive, watch } from 'vue'

const state = reactive({ count: 0 })
watch(
  () => state.count,
  (count) => {
    console.log(count)
  },
)
</script>
```

## Detection

Fact-driven: `WatchSources` reads whose `property` is the deep-root sentinel `*`.

## Remediation

Watch a getter or a concrete list of sources instead of the whole reactive root.
