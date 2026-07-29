# `vue-vet/correctness/valid-v-memo`

Category: correctness  
Default severity: error  
Confidence: high

`v-memo` must include a non-empty expression.

## Bad

```vue
<template>
  <div v-memo="">content</div>
</template>
```

## Good

```vue
<script setup lang="ts">
import { ref } from 'vue'
const value = ref(true)
</script>

<template>
  <div v-memo="value">content</div>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Provide an expression for `v-memo`.

## Fixtures

- Invalid: `fixtures/rules/valid-v-memo/invalid/`
- Valid: `fixtures/rules/valid-v-memo/valid/`
