# `vue-vet/accessibility/button-has-content`

Category: accessibility  
Default severity: warning  
Confidence: high

Vue Vet rule `vue-vet/accessibility/button-has-content` reports a fact-driven correctness or reactivity issue. Prefer the Bad/Good examples below; fixtures under `fixtures/rules/button-has-content/` are the executable corpus.

## Bad

```vue
<template>
  <button type="button">
    <div class="i-carbon-close text-xl" />
  </button>
</template>
```

## Good

```vue
<template>
  <button type="button" aria-label="Close">
    <div class="i-carbon-close text-xl" />
  </button>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Follow the Good pattern, or suppress with a narrow inline disable when reviewed.

## Fixtures

- Invalid: `fixtures/rules/button-has-content/invalid/`
- Valid: `fixtures/rules/button-has-content/valid/`
