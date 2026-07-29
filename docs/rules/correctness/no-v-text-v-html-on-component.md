# `vue-vet/correctness/no-v-text-v-html-on-component`

Category: correctness  
Default severity: warning  
Confidence: high

`v-text` / `v-html` on components do not reliably set component content.

## Bad

```vue
<template>
  <Comp v-html="html" />
</template>
```

## Good

```vue
<template>
  <div v-html="html" />
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Apply `v-html` / `v-text` to native elements, or pass content through slots/props.

## Fixtures

- Invalid: `fixtures/rules/no-v-text-v-html-on-component/invalid/`
- Valid: `fixtures/rules/no-v-text-v-html-on-component/valid/`
