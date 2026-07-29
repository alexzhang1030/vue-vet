# `vue-vet/correctness/valid-v-for`

Category: correctness  
Default severity: error  
Confidence: high

`v-for` must include a non-empty expression.

## Bad

```vue
<template>
  <li v-for="">{{ item }}</li>
</template>
```

## Good

```vue
<script setup lang="ts">
import { ref } from 'vue'
const items = ref(['a', 'b', 'c'])
</script>
<template>
  <li v-for="item in items" :key="item">{{ item }}</li>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Provide an expression for `v-for`.

## Fixtures

- Invalid: `fixtures/rules/valid-v-for/invalid/`
- Valid: `fixtures/rules/valid-v-for/valid/`
