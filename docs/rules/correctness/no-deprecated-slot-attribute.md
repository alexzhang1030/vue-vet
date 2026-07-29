# `vue-vet/correctness/no-deprecated-slot-attribute`

Category: correctness  
Default severity: warning  
Confidence: high

The `slot` attribute is Vue 2 syntax. Prefer `v-slot` / `#`.

## Bad

```vue
<template>
  <Comp><div slot="header">Title</div></Comp>
</template>
```

## Good

```vue
<template>
  <Comp><template #header>Title</template></Comp>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Migrate to `v-slot` / named slot shorthand.

## Fixtures

- Invalid: `fixtures/rules/no-deprecated-slot-attribute/invalid/`
- Valid: `fixtures/rules/no-deprecated-slot-attribute/valid/`
