# `vue-vet/correctness/valid-v-bind`

Category: correctness  
Default severity: error  
Confidence: high

`v-bind` / `:` requires an expression (unless using the object form correctly).

## Bad

```vue
<template>
  <div v-bind="">Nothing bound</div>
</template>
```

## Good

```vue
<script setup lang="ts">
import { ref } from 'vue'
const id = ref('main')
</script>
<template>
  <div v-bind:id="id">Bound id</div>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Provide a binding expression.

## Fixtures

- Invalid: `fixtures/rules/valid-v-bind/invalid/`
- Valid: `fixtures/rules/valid-v-bind/valid/`
