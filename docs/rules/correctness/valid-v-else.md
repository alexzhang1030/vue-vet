# `vue-vet/correctness/valid-v-else`

Category: correctness  
Default severity: error  
Confidence: high

`v-else` must immediately follow a `v-if` / `v-else-if` chain.

## Bad

```vue
<script setup lang="ts">
import { ref } from 'vue'
const ok = ref(true)
</script>
<template>
  <div v-if="ok">A</div>
  <div v-else="ok">B</div>
</template>
```

## Good

```vue
<script setup lang="ts">
import { ref } from 'vue'
const ok = ref(true)
</script>
<template>
  <div v-if="ok">A</div>
  <div v-else>B</div>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Attach `v-else` to a valid chain.

## Fixtures

- Invalid: `fixtures/rules/valid-v-else/invalid/`
- Valid: `fixtures/rules/valid-v-else/valid/`
