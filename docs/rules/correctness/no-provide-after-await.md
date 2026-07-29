# `vue-vet/correctness/no-provide-after-await`

Category: correctness  
Default severity: warning  
Confidence: high

In `<script setup>`, calling `provide` after a top-level `await` runs outside the synchronous setup instance context, so the API will not bind correctly.

## Bad

```vue
<script setup lang="ts">
import { provide } from 'vue'
const data = await fetch('/api').then((response) => response.json())
const value = provide('key', 1)
</script>

<template>
  <div>{{ data }} {{ value }}</div>
</template>
```

## Good

```vue
<script setup lang="ts">
import { provide } from 'vue'
provide('key', 1)
const data = await fetch('/api').then((response) => response.json())
</script>

<template>
  <div>{{ data }}</div>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Move `provide` before the first top-level `await`.

## Fixtures

- Invalid: `fixtures/rules/no-provide-after-await/invalid/`
- Valid: `fixtures/rules/no-provide-after-await/valid/`
