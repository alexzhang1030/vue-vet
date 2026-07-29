# `vue-vet/correctness/no-define-emits-after-await`

Category: correctness  
Default severity: warning  
Confidence: high

In `<script setup>`, calling `defineEmits` after a top-level `await` runs outside the synchronous setup instance context, so the API will not bind correctly.

## Bad

```vue
<script setup lang="ts">
const data = await fetch('/api').then((response) => response.json())
defineEmits()
</script>

<template>
  <div>{{ data }}</div>
</template>
```

## Good

```vue
<script setup lang="ts">
defineEmits()
const data = await fetch('/api').then((response) => response.json())
</script>

<template>
  <div>{{ data }}</div>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Move `defineEmits` before the first top-level `await`.

## Fixtures

- Invalid: `fixtures/rules/no-define-emits-after-await/invalid/`
- Valid: `fixtures/rules/no-define-emits-after-await/valid/`
