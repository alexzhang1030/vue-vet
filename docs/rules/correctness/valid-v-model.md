# `vue-vet/correctness/valid-v-model`

Category: correctness  
Default severity: error  
Confidence: high

`v-model` must include a non-empty expression.

## Bad

```vue
<template>
  <input v-model="" />
</template>
```

## Good

```vue
<script setup lang="ts">
import { ref } from 'vue'
const text = ref('')
</script>
<template>
  <input v-model="text" />
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Provide an expression for `v-model`.

## Fixtures

- Invalid: `fixtures/rules/valid-v-model/invalid/`
- Valid: `fixtures/rules/valid-v-model/valid/`
