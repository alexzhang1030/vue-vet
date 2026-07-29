# `vue-vet/correctness/no-duplicate-attributes`

Category: correctness  
Default severity: warning  
Confidence: high

Duplicate attributes on the same element are ambiguous.

## Bad

```vue
<template>
  <div id="a" id="b" />
</template>
```

## Good

```vue
<template>
  <div id="a" />
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Keep a single attribute of each name (or merge bindings intentionally).

## Fixtures

- Invalid: `fixtures/rules/no-duplicate-attributes/invalid/`
- Valid: `fixtures/rules/no-duplicate-attributes/valid/`
