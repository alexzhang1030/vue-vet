# `vue-vet/reactivity/no-ref-as-operand`

Category: reactivity  
Default severity: warning  
Confidence: high

Using a ref object directly as an operand reads the object wrapper, not the inner value. Unwrap with `.value` (or `toValue`).

## Bad

```vue
<script setup lang="ts">
import { ref } from 'vue'
const count = ref(0)
const ok = count > 0
</script>
<template>{{ ok }}</template>
```

## Good

```vue
<script setup lang="ts">
import { ref } from 'vue'
const count = ref(0)
const ok = count.value > 0
</script>
<template>{{ ok }}</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Read `ref.value` (or `toValue(...)`) at the use site.

## Fixtures

- Invalid: `fixtures/rules/no-ref-as-operand/invalid/`
- Valid: `fixtures/rules/no-ref-as-operand/valid/`
