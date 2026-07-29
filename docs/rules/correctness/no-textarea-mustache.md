# `vue-vet/correctness/no-textarea-mustache`

Category: correctness  
Default severity: warning  
Confidence: high

Interpolation inside `<textarea>` is not the Vue 3 control surface; bind with `v-model`.

## Bad

```vue
<template>
  <textarea>{{ text }}</textarea>
</template>
```

## Good

```vue
<template>
  <textarea v-model="text" />
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Use `v-model` (or `:value` + listeners) instead of mustache children.

## Fixtures

- Invalid: `fixtures/rules/no-textarea-mustache/invalid/`
- Valid: `fixtures/rules/no-textarea-mustache/valid/`
