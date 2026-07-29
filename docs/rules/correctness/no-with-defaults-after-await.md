# `vue-vet/correctness/no-with-defaults-after-await`

Category: correctness  
Default severity: warning  
Confidence: high

In `<script setup>`, calling `withDefaults` after a top-level `await` runs outside the synchronous setup instance context, so the API will not bind correctly.

## Bad

```vue
<script setup lang="ts">
const data = await fetch('/api').then((response) => response.json())
const props = withDefaults(defineProps<{ title?: string }>(), { title: 'hi' })
</script>

<template>
  <p>{{ props.title }} {{ data }}</p>
</template>
```

## Good

```vue
<script setup lang="ts">
const props = withDefaults(defineProps<{ title?: string }>(), { title: 'hi' })
const data = await fetch('/api').then((response) => response.json())
</script>

<template>
  <p>{{ props.title }} {{ data }}</p>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Move `withDefaults` before the first top-level `await`.

## Fixtures

- Invalid: `fixtures/rules/no-with-defaults-after-await/invalid/`
- Valid: `fixtures/rules/no-with-defaults-after-await/valid/`
