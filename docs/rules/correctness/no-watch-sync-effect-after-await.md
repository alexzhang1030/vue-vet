# `vue-vet/correctness/no-watch-sync-effect-after-await`

Category: correctness  
Default severity: warning  
Confidence: high

In `<script setup>`, calling `watchSyncEffect` after a top-level `await` runs outside the synchronous setup instance context, so the API will not bind correctly.

## Bad

```vue
<script setup lang="ts">
import { watchSyncEffect } from 'vue'
const data = await fetch('/api').then((response) => response.json())
watchSyncEffect(() => {
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
import { watchSyncEffect } from 'vue'
watchSyncEffect(() => {
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

Move `watchSyncEffect` before the first top-level `await`.

## Fixtures

- Invalid: `fixtures/rules/no-watch-sync-effect-after-await/invalid/`
- Valid: `fixtures/rules/no-watch-sync-effect-after-await/valid/`
