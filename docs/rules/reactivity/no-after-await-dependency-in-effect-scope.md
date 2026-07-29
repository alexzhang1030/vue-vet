# `vue-vet/reactivity/no-after-await-dependency-in-effect-scope`

Category: reactivity  
Default severity: warning  
Confidence: high

Reports reactive reads inside `effectScope` that happen after `await`. Those reads are not stable dependencies for the tracking scope.

## Bad

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const label = computed(async () => {
  await Promise.resolve()
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
const count = ref(0)
const label = computed(() => String(count.value))
</script>

<template>
  <p>{{ label }}</p>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Keep reactive reads synchronous and unconditional inside `effectScope`, or switch to an API with explicit sources (`watch([...])`).

## Fixtures

- Invalid: `fixtures/rules/no-after-await-dependency-in-effect-scope/invalid/`
- Valid: `fixtures/rules/no-after-await-dependency-in-effect-scope/valid/`
