# `vue-vet/reactivity/no-conditional-dependency-in-computed`

Category: reactivity  
Default severity: warning  
Confidence: high

Reports reactive reads inside `computed` that happen only after a control-flow guard. Those reads are not stable dependencies for the tracking scope.

## Bad

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'
const enabled = ref(false)
const count = ref(0)
const label = computed(() => {
  if (!enabled.value) return 'off'
  return String(count.value)
})
</script>

<template>
  <p>{{ label }}</p>
</template>
```

## Good

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'
const enabled = ref(false)
const count = ref(0)
const label = computed(() => (enabled.value ? String(count.value) : 'off'))
</script>

<template>
  <p>{{ label }}</p>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Keep reactive reads synchronous and unconditional inside `computed`, or switch to an API with explicit sources (`watch([...])`).

## Fixtures

- Invalid: `fixtures/rules/no-conditional-dependency-in-computed/invalid/`
- Valid: `fixtures/rules/no-conditional-dependency-in-computed/valid/`
