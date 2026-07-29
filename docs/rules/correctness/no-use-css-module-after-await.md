# `vue-vet/correctness/no-use-css-module-after-await`

Category: correctness  
Default severity: warning  
Confidence: high

In `<script setup>`, calling `useCssModule` after a top-level `await` runs outside the synchronous setup instance context, so the API will not bind correctly.

## Bad

```vue
<script setup lang="ts">
import { useCssModule } from 'vue'
const data = await fetch('/api').then((response) => response.json())
const value = useCssModule()
</script>

<template>
  <div>{{ data }} {{ value }}</div>
</template>
```

## Good

```vue
<script setup lang="ts">
import { useCssModule } from 'vue'
const value = useCssModule()
const data = await fetch('/api').then((response) => response.json())
</script>

<template>
  <div>{{ data }}</div>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Move `useCssModule` before the first top-level `await`.

## Fixtures

- Invalid: `fixtures/rules/no-use-css-module-after-await/invalid/`
- Valid: `fixtures/rules/no-use-css-module-after-await/valid/`
