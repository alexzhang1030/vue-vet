# `vue-vet/correctness/valid-v-show`

Category: correctness  
Default severity: error  
Confidence: high

`v-show` must include a non-empty expression.

## Bad

```vue
<template>
  <div v-show="">Never toggled correctly</div>
</template>
```

## Good

```vue
<script setup lang="ts">
import { ref } from 'vue'
const visible = ref(true)
</script>
<template>
  <div v-show="visible">Toggled by display</div>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Provide an expression for `v-show`.

## Fixtures

- Invalid: `fixtures/rules/valid-v-show/invalid/`
- Valid: `fixtures/rules/valid-v-show/valid/`
