# `vue-vet/reactivity/prefer-store-to-refs`

Category: reactivity  
Default severity: warning  
Confidence: high

Destructuring a Pinia store loses reactivity for state fields

## Bad

```vue
<script setup lang="ts">
import { reactive } from 'vue'
const { count } = reactive({ count: 0 })
</script>

<template>
  <p>{{ count }}</p>
</template>
```

## Good

```vue
<script setup lang="ts">
import { toRefs } from 'vue'
// Keep the reactive object and read through it, or use toRefs / storeToRefs.
const state = /* reactive object */ ({ count: 0 } as any)
const { count } = toRefs(state)
</script>

<template>
  <p>{{ count }}</p>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Use `storeToRefs(store)` for state/getters, and keep actions on the store object.

## Fixtures

- Invalid: `fixtures/rules/prefer-store-to-refs/invalid/`
- Valid: `fixtures/rules/prefer-store-to-refs/valid/`
