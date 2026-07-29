# `vue-vet/reactivity/no-shallow-reactive-destructure`

Category: reactivity  
Default severity: warning  
Confidence: high

Destructuring `shallowReactive()` loses reactivity for the pulled fields

## Bad

```vue
<script setup lang="ts">
import { reactive } from 'vue'
const { count } = shallowReactive()
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
const state = /* shallowReactive */ ({ count: 0 } as any)
const { count } = toRefs(state)
</script>

<template>
  <p>{{ count }}</p>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Keep the shallowReactive object, or use `toRefs` / `toRef`.

## Fixtures

- Invalid: `fixtures/rules/no-shallow-reactive-destructure/invalid/`
- Valid: `fixtures/rules/no-shallow-reactive-destructure/valid/`
