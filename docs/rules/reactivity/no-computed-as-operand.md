# `vue-vet/reactivity/no-computed-as-operand`

Category: reactivity  
Default severity: warning  
Confidence: high

Using a computed ref object directly as an operand reads the object wrapper, not the inner value. Unwrap with `.value` (or `toValue`).

## Bad

```vue
<script setup lang="ts">
import { ref } from 'vue'
const count = ref(0)
const ok = count > 0
</script>

<template>
  <p>{{ ok }}</p>
</template>
```

## Good

```vue
<script setup lang="ts">
import { ref } from 'vue'
const count = ref(0)
const ok = count.value > 0
</script>

<template>
  <p>{{ ok }}</p>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Read `computed ref.value` (or `toValue(...)`) at the use site.

## Fixtures

- Invalid: `fixtures/rules/no-computed-as-operand/invalid/`
- Valid: `fixtures/rules/no-computed-as-operand/valid/`
