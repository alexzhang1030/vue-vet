# `vue-vet/correctness/no-child-content`

Category: correctness  
Default severity: warning  
Confidence: high

Elements that take text via an attribute (for example `v-text` / `v-html` / `textarea` value patterns) should not also carry child content that the attribute replaces.

## Bad

```vue
<template>
  <div v-text="msg">also children</div>
</template>
```

## Good

```vue
<template>
  <div v-text="msg" />
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Remove the child nodes, or drop the attribute and keep children.

## Fixtures

- Invalid: `fixtures/rules/no-child-content/invalid/`
- Valid: `fixtures/rules/no-child-content/valid/`
