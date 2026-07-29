# `vue-vet/correctness/no-get-current-instance-after-await`

Category: correctness  
Default severity: warning  
Confidence: high

In `<script setup>`, calling `getCurrentInstance` after a top-level `await` runs outside the synchronous setup instance context, so the API will not bind correctly.

## Bad

```vue
<script setup lang="ts">
import { getCurrentInstance } from 'vue'
const data = await fetch('/api').then((response) => response.json())
const value = getCurrentInstance()
</script>

<template>
  <div>{{ data }} {{ value }}</div>
</template>
```

## Good

```vue
<script setup lang="ts">
import { getCurrentInstance } from 'vue'
const value = getCurrentInstance()
const data = await fetch('/api').then((response) => response.json())
</script>

<template>
  <div>{{ data }}</div>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Move `getCurrentInstance` before the first top-level `await`.

## Fixtures

- Invalid: `fixtures/rules/no-get-current-instance-after-await/invalid/`
- Valid: `fixtures/rules/no-get-current-instance-after-await/valid/`
