# `vue-vet/correctness/no-use-slots-after-await`

Category: correctness  
Default severity: warning  
Confidence: high

In `<script setup>`, calling `useSlots` after a top-level `await` runs outside the synchronous setup instance context, so the API will not bind correctly.

## Bad

```vue
<script setup lang="ts">
import { useSlots } from 'vue'
const data = await fetch('/api').then((response) => response.json())
const value = useSlots()
</script>

<template>
  <div>{{ data }} {{ value }}</div>
</template>
```

## Good

```vue
<script setup lang="ts">
import { useSlots } from 'vue'
const value = useSlots()
const data = await fetch('/api').then((response) => response.json())
</script>

<template>
  <div>{{ data }}</div>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Move `useSlots` before the first top-level `await`.

## Fixtures

- Invalid: `fixtures/rules/no-use-slots-after-await/invalid/`
- Valid: `fixtures/rules/no-use-slots-after-await/valid/`
