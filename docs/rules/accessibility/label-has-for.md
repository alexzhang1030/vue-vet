# `vue-vet/accessibility/label-has-for`

Category: accessibility  
Default severity: warning  
Confidence: high

Vue Vet rule `vue-vet/accessibility/label-has-for` reports a fact-driven correctness or reactivity issue. Prefer the Bad/Good examples below; fixtures under `fixtures/rules/label-has-for/` are the executable corpus.

## Bad

```vue
<template>
  <label>Email</label>
  <input id="email" type="email" aria-label="Email">
</template>
```

## Good

```vue
<template>
  <label for="email">Email</label>
  <input id="email" type="email">
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Follow the Good pattern, or suppress with a narrow inline disable when reviewed.

## Fixtures

- Invalid: `fixtures/rules/label-has-for/invalid/`
- Valid: `fixtures/rules/label-has-for/valid/`
