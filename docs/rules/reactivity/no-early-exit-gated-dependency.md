# `vue-vet/reactivity/no-early-exit-gated-dependency`

Category: reactivity  
Default severity: warning  
Confidence: high  
Tier: tracer

A reactive read reached only after an early-exit guard (`if (…) return`) is a
conditional dependency. Vue may not track it on runs that take the early path.

## Bad

```vue
<script setup lang="ts">
import { ref, watchEffect } from 'vue'

const enabled = ref(false)
const result = ref(0)
watchEffect(() => {
  if (!enabled.value) return
  console.log(result.value)
})
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref, watchEffect } from 'vue'

const enabled = ref(false)
const result = ref(0)
watchEffect(() => {
  const current = result.value
  if (!enabled.value) return
  console.log(current)
})
</script>
```

## Detection

Fact-driven: `Conditional` reads whose guards include `ReactiveGuardRole::EarlyExit`.

## Remediation

Read the dependency before the early return, or use explicit `watch` sources.
