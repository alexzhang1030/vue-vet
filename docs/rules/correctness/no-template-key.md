# `vue-vet/correctness/no-template-key`

Category: correctness  
Default severity: warning  
Confidence: high

`<template>` special elements should not carry `key` the way elements do; put `key` on real elements / `v-for` sources as Vue expects.

## Bad

```vue
<template>
  <template key="x"><div /></template>
</template>
```

## Good

```vue
<template>
  <div key="x" />
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Move `key` onto the keyed element or the `v-for` node Vue documents.

## Fixtures

- Invalid: `fixtures/rules/no-template-key/invalid/`
- Valid: `fixtures/rules/no-template-key/valid/`
