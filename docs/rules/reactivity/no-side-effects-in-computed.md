# `vue-vet/reactivity/no-side-effects-in-computed`

Category: reactivity  
Default severity: warning  
Confidence: high

`computed` getters should be pure. Side effects belong in `watch` / lifecycle hooks.

## Bad

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'
const a = ref(0)
const b = ref(0)
const c = computed(() => { b.value = a.value; return a.value })
</script>
<template>{{ c }}</template>
```

## Good

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'
const a = ref(0)
const c = computed(() => a.value + 1)
</script>
<template>{{ c }}</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Move side effects out of the computed getter.

## Fixtures

- Invalid: `fixtures/rules/no-side-effects-in-computed/invalid/`
- Valid: `fixtures/rules/no-side-effects-in-computed/valid/`
