# `vue-vet/reactivity/no-stale-prop-flow`

Category: reactivity  
Default severity: warning  
Confidence: high

Cross-file prop edges should start from reactive parent state; plain values go stale.

## Bad

```vue
<!-- parent -->
<script setup lang="ts">
let title = 'hi'
</script>
<template>
  <Child :title="title" />
</template>
```

## Good

```vue
<!-- parent -->
<script setup lang="ts">
import { ref } from 'vue'
const title = ref('hi')
</script>
<template>
  <Child :title="title" />
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Pass a reactive binding (ref/computed/reactive field).

## Fixtures

- Invalid: `fixtures/rules/no-stale-prop-flow/invalid/`
- Valid: `fixtures/rules/no-stale-prop-flow/valid/`
