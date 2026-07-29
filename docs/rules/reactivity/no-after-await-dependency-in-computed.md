# `vue-vet/reactivity/no-after-await-dependency-in-computed`

Category: reactivity  
Default severity: warning  
Confidence: high

Reports reactive reads inside `computed` that happen after `await`. Those reads are not stable dependencies for the tracking scope.

## Bad

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'
const a = ref(0)
const c = computed(async () => {
  await Promise.resolve()
  return a.value
})
</script>
<template>{{ c }}</template>
```

## Good

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'
const a = ref(0)
const c = computed(() => a.value)
</script>
<template>{{ c }}</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Keep reactive reads synchronous and unconditional inside `computed`, or switch to an API with explicit sources (`watch([...])`).

## Fixtures

- Invalid: `fixtures/rules/no-after-await-dependency-in-computed/invalid/`
- Valid: `fixtures/rules/no-after-await-dependency-in-computed/valid/`
