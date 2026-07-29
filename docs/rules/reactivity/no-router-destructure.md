# `vue-vet/reactivity/no-router-destructure`

Category: reactivity  
Default severity: warning  
Confidence: high

Destructuring `useRouter()` is usually unnecessary and can hide API misuse

## Bad

```vue
<script setup lang="ts">
import { reactive } from 'vue'
const { count } = useRouter()
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
const state = /* useRouter */ ({ count: 0 } as any)
const { count } = toRefs(state)
</script>

<template>
  <p>{{ count }}</p>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Keep the router instance and call methods on it.

## Fixtures

- Invalid: `fixtures/rules/no-router-destructure/invalid/`
- Valid: `fixtures/rules/no-router-destructure/valid/`
