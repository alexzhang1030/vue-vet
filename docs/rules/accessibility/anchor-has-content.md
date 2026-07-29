# `vue-vet/accessibility/anchor-has-content`

Category: accessibility  
Default severity: warning  
Confidence: high

Vue Vet rule `vue-vet/accessibility/anchor-has-content` reports a fact-driven correctness or reactivity issue. Prefer the Bad/Good examples below; fixtures under `fixtures/rules/anchor-has-content/` are the executable corpus.

## Bad

```vue
<template>
  <a href="/help">
    <span aria-hidden="true">Help</span>
  </a>
</template>
```

## Good

```vue
<template>
  <a href="https://github.com" aria-label="GitHub">
    <div class="i-carbon-logo-github text-xl" />
  </a>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Follow the Good pattern, or suppress with a narrow inline disable when reviewed.

## Fixtures

- Invalid: `fixtures/rules/anchor-has-content/invalid/`
- Valid: `fixtures/rules/anchor-has-content/valid/`
