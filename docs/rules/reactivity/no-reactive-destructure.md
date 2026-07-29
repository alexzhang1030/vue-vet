# `vue-vet/reactivity/no-reactive-destructure`

Category: reactivity  
Default severity: warning  
Confidence: high

Destructuring `reactive()` loses reactivity for the pulled fields

## Bad

```vue
<script setup lang="ts">
import { reactive } from 'vue'
const { count } = reactive({ count: 0 })
</script>
<template>{{ count }}</template>
```

## Good

```vue
<script setup lang="ts">
import { reactive } from 'vue'
const state = reactive({ count: 0 })
</script>
<template>{{ state.count }}</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Keep the reactive object, or use `toRefs` / `toRef`.

## Fixtures

- Invalid: `fixtures/rules/no-reactive-destructure/invalid/`
- Valid: `fixtures/rules/no-reactive-destructure/valid/`
