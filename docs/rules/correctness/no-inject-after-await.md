# `vue-vet/correctness/no-inject-after-await`

Category: correctness  
Default severity: warning  
Confidence: high

In `<script setup>`, calling `inject` after a top-level `await` runs outside the synchronous setup instance context, so the API will not bind correctly.

## Bad

```vue
<script setup lang="ts">
import { inject } from 'vue'
const data = await fetch('/api').then((response) => response.json())
const value = inject('key')
</script>

<template>
  <div>{{ data }} {{ value }}</div>
</template>
```

## Good

```vue
<script setup lang="ts">
import { inject } from 'vue'
const value = inject('key')
const data = await fetch('/api').then((response) => response.json())
</script>

<template>
  <div>{{ data }} {{ value }}</div>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Move `inject` before the first top-level `await`.

## Fixtures

- Invalid: `fixtures/rules/no-inject-after-await/invalid/`
- Valid: `fixtures/rules/no-inject-after-await/valid/`
