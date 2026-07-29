# `vue-vet/correctness/valid-v-if`

Category: correctness  
Default severity: error  
Confidence: high

`v-if` must include a non-empty expression.

## Bad

```vue
<template>
  <div v-if="">Never shown</div>
</template>
```

## Good

```vue
<script setup lang="ts">
import { ref } from 'vue'
const visible = ref(true)
</script>
<template>
  <div v-if="visible">Shown when visible</div>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Provide an expression for `v-if`.

## Fixtures

- Invalid: `fixtures/rules/valid-v-if/invalid/`
- Valid: `fixtures/rules/valid-v-if/valid/`
