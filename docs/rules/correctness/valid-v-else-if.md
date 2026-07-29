# `vue-vet/correctness/valid-v-else-if`

Category: correctness  
Default severity: error  
Confidence: high

`v-else-if` must include a non-empty expression.

## Bad

```vue
<script setup lang="ts">
import { ref } from 'vue'
const kind = ref('a')
</script>
<template>
  <div v-if="kind === 'a'">A</div>
  <div v-else-if="">B</div>
</template>
```

## Good

```vue
<script setup lang="ts">
import { ref } from 'vue'
const kind = ref('a')
</script>
<template>
  <div v-if="kind === 'a'">A</div>
  <div v-else-if="kind === 'b'">B</div>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Provide an expression for `v-else-if`.

## Fixtures

- Invalid: `fixtures/rules/valid-v-else-if/invalid/`
- Valid: `fixtures/rules/valid-v-else-if/valid/`
