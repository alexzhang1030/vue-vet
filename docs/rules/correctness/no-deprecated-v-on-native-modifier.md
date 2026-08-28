# `vue-vet/correctness/no-deprecated-v-on-native-modifier`

Category: correctness  
Default severity: error  
Confidence: high

The `.native` event modifier was removed in Vue 3. Undeclared listeners fall
through to the native element.

## Bad

```vue
<template>
  <Widget @click.native="activate" />
</template>
```

## Good

```vue
<template>
  <Widget @click="activate" />
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Limitations

`--fix-safe` reconstructs the contiguous `@event.native` / `v-on:event.native`
name from Vize's `@` / `v-on` prefix span, then drops `.native`. The handler
value is left untouched, so extra modifiers such as `@click.native.stop`
become `@click.stop`. A prefix that does not match source, a non-contiguous
name, or a dangling `@` / `v-on:` after the strip stays diagnostic-only.

## Remediation

Remove `.native` and rely on Vue 3 listener fallthrough. `--fix-safe` applies
the reconstructable name form.

## Fixtures

- Invalid: `fixtures/rules/no-deprecated-v-on-native-modifier/invalid/`
- Valid: `fixtures/rules/no-deprecated-v-on-native-modifier/valid/`
