# `vue-vet/reactivity/no-short-circuit-gated-dependency`

Category: reactivity  
Default severity: warning  
Confidence: high  
Tier: tracer

The right-hand side of `&&` / `||` is only evaluated when the left-hand side
allows it. Reactive reads there are short-circuit-gated dependencies.

## Bad

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'

const enabled = ref(false)
const count = ref(0)
const label = computed(() => enabled.value && String(count.value))
</script>
```

## Good

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'

const enabled = ref(false)
const count = ref(0)
const label = computed(() => {
  const n = count.value
  return enabled.value ? String(n) : 'off'
})
</script>
```

## Detection

Fact-driven: `Conditional` reads with `ReactiveGuardRole::ShortCircuit`.

## Remediation

Read every required dependency unconditionally, or use explicit `watch` sources.
