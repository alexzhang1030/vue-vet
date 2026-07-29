# `vue-vet/reactivity/no-v-model-nonreactive-source`

Category: reactivity  
Default severity: warning  
Confidence: high

`v-model` should bind a reactive script value.

## Bad

```vue
<script setup lang="ts">
let text = ''
</script>
<template>
  <input v-model="text" />
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

Bind a `ref` / `computed` / reactive property.

## Fixtures

- Invalid: `fixtures/rules/no-v-model-nonreactive-source/invalid/`
- Valid: `fixtures/rules/no-v-model-nonreactive-source/valid/`
