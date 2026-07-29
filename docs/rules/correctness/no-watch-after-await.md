# `vue-vet/correctness/no-watch-after-await`

Category: correctness  
Default severity: warning  
Confidence: high

In `<script setup>`, calling `watch` after a top-level `await` runs outside the synchronous setup instance context, so the API will not bind correctly.

## Bad

```vue
<script setup lang="ts">
import { watch } from 'vue'
const data = await fetch('/api').then((response) => response.json())
watch(() => {
  console.log(data)
})
</script>

<template>
  <div />
</template>
```

## Good

```vue
<script setup lang="ts">
import { watch } from 'vue'
watch(() => {
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

Move `watch` before the first top-level `await`.

## Fixtures

- Invalid: `fixtures/rules/no-watch-after-await/invalid/`
- Valid: `fixtures/rules/no-watch-after-await/valid/`
