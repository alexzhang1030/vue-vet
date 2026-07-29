# `vue-vet/reactivity/no-route-destructure`

Category: reactivity  
Default severity: warning  
Confidence: high

Destructuring `useRoute()` loses reactivity

## Bad

```vue
<script setup lang="ts">
import { reactive } from 'vue'
const { count } = useRoute()
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
const state = /* useRoute */ ({ count: 0 } as any)
const { count } = toRefs(state)
</script>

<template>
  <p>{{ count }}</p>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Keep the route object or read `route.params` / `route.query` through it.

## Fixtures

- Invalid: `fixtures/rules/no-route-destructure/invalid/`
- Valid: `fixtures/rules/no-route-destructure/valid/`
