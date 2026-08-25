# `vue-vet/correctness/no-deprecated-v-bind-sync`

Category: correctness  
Default severity: error  
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

## Limitations

`--fix-safe` rewrites only quoted `:arg.sync="expr"` and `v-bind:arg.sync="expr"`
when the argument is a static prop name and `.sync` is the only modifier.
Object `v-bind.sync="state"`, unquoted values, extra modifiers, and dynamic
`:[name].sync` stay diagnostic-only.

## Remediation

Replace `.sync` with `v-model` arguments. `--fix-safe` applies the quoted
named-argument form.

## Fixtures

- Invalid: `fixtures/rules/no-deprecated-v-bind-sync/invalid/`
- Valid: `fixtures/rules/no-deprecated-v-bind-sync/valid/`
