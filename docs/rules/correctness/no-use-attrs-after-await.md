# `vue-vet/correctness/no-use-attrs-after-await`

Category: correctness  
Default severity: warning  
Confidence: high

In `<script setup>`, calling `useAttrs` after a top-level `await` runs outside the synchronous setup instance context, so the API will not bind correctly.

## Bad

```vue
<script setup lang="ts">
import { useAttrs } from 'vue'
const data = await fetch('/api').then((response) => response.json())
const value = useAttrs()
</script>

<template>
  <div>{{ data }} {{ value }}</div>
</template>
```

## Good

```vue
<script setup lang="ts">
import { useAttrs } from 'vue'
const value = useAttrs()
const data = await fetch('/api').then((response) => response.json())
</script>

<template>
  <div>{{ data }}</div>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Move `useAttrs` before the first top-level `await`.

## Fixtures

- Invalid: `fixtures/rules/no-use-attrs-after-await/invalid/`
- Valid: `fixtures/rules/no-use-attrs-after-await/valid/`
