# `vue-vet/correctness/no-effect-scope-after-await`

Category: correctness  
Default severity: warning  
Confidence: high

In `<script setup>`, calling `effectScope` after a top-level `await` runs outside the synchronous setup instance context, so the API will not bind correctly.

## Bad

```vue
<script setup lang="ts">
import { effectScope } from 'vue'
const data = await fetch('/api').then((response) => response.json())
const value = effectScope()
</script>

<template>
  <div>{{ data }} {{ value }}</div>
</template>
```

## Good

```vue
<script setup lang="ts">
import { effectScope } from 'vue'
effectScope(() => {
  console.log('ready')
})
const data = await fetch('/api').then((response) => response.json())
</script>

<template>
  <div>{{ data }}</div>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Move `effectScope` before the first top-level `await`.

## Fixtures

- Invalid: `fixtures/rules/no-effect-scope-after-await/invalid/`
- Valid: `fixtures/rules/no-effect-scope-after-await/valid/`
