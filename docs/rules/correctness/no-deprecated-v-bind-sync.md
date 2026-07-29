# `vue-vet/correctness/no-deprecated-v-bind-sync`

Category: correctness  
Default severity: warning  
Confidence: high

`.sync` is Vue 2 sugar. Prefer `v-model:prop` in Vue 3.

## Bad

```vue
<template>
  <Comp :title.sync="title" />
</template>
```

## Good

```vue
<template>
  <Comp v-model:title="title" />
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Replace `.sync` with `v-model` arguments.

## Fixtures

- Invalid: `fixtures/rules/no-deprecated-v-bind-sync/invalid/`
- Valid: `fixtures/rules/no-deprecated-v-bind-sync/valid/`
