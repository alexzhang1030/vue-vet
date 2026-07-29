# `vue-vet/correctness/valid-v-on`

Category: correctness  
Default severity: error  
Confidence: high

`v-on` / `@` needs an event name and a handler expression.

## Bad

```vue
<template>
  <button v-on="">Click</button>
</template>
```

## Good

```vue
<script setup lang="ts">
function onClick() {}
</script>
<template>
  <button v-on:click="onClick">Click</button>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Provide an event name and handler.

## Fixtures

- Invalid: `fixtures/rules/valid-v-on/invalid/`
- Valid: `fixtures/rules/valid-v-on/valid/`
