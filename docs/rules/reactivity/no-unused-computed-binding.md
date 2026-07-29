# `vue-vet/reactivity/no-unused-computed-binding`

Category: reactivity  
Default severity: warning  
Confidence: high

A `computed` binding that is never read is dead work.

## Bad

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const doubled = computed(() => count.value * 2)
</script>
<template>
  <p>{{ count }}</p>
</template>
```

## Good

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const doubled = computed(() => count.value * 2)
</script>
<template>
  <p>{{ doubled }}</p>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Read the computed in script/template, or delete it.

## Fixtures

- Invalid: `fixtures/rules/no-unused-computed-binding/invalid/`
- Valid: `fixtures/rules/no-unused-computed-binding/valid/`
