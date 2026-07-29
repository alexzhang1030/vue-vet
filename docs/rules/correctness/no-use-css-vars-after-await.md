# `vue-vet/correctness/no-use-css-vars-after-await`

Category: correctness  
Default severity: warning  
Confidence: high

In `<script setup>`, calling `useCssVars` after a top-level `await` runs outside the synchronous setup instance context, so the API will not bind correctly.

## Bad

```vue
<script setup lang="ts">
import { useCssVars } from 'vue'
const data = await fetch('/api').then((response) => response.json())
const value = useCssVars()
</script>

<template>
  <div>{{ data }} {{ value }}</div>
</template>
```

## Good

```vue
<script setup lang="ts">
import { useCssVars } from 'vue'
const value = useCssVars()
const data = await fetch('/api').then((response) => response.json())
</script>

<template>
  <div>{{ data }}</div>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Move `useCssVars` before the first top-level `await`.

## Fixtures

- Invalid: `fixtures/rules/no-use-css-vars-after-await/invalid/`
- Valid: `fixtures/rules/no-use-css-vars-after-await/valid/`
