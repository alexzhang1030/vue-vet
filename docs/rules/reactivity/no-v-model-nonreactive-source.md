# `vue-vet/reactivity/no-v-model-nonreactive-source`

Category: reactivity  
Default severity: warning  
Confidence: high

`v-model` should bind a reactive script value.

## Bad

```vue
<script setup lang="ts">
let text = ''
</script>
<template>
  <input v-model="text" />
</template>
```

## Good

```vue
<script setup lang="ts">
import { ref } from 'vue'
const text = ref('')
</script>
<template>
  <input v-model="text" />
</template>
```

Also quiet for compiler-macro model refs:

```vue
<script setup lang="ts">
const text = defineModel<string>()
// Vue Macros: const { modelValue } = defineModels<{ modelValue: string }>()
</script>
<template>
  <input v-model="text" />
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).
`defineModel` and Vue Macros `defineModels` destructured locals seed `ModelRef` bindings in
`<script setup>` only.
Absence of a graph classification is **unknown**, not nonreactive. The rule
reports only when Oxc records a **plain initializer** (`let text = ''`,
uninitialized `let`) **and** the binding has no later script reassignment.
`let form; form = createForm()` stays unknown. Global `undefined` is a plain
initializer only when Oxc resolves it as the unresolved global, not a
shadowed import. Member copies (`const form = source.form`) and
conditionals (`condition ? createForm() : otherForm`) stay unknown.

## Remediation

Bind a `ref` / `computed` / reactive property, or use `defineModel` / `defineModels`.

## Fixtures

- Invalid: `fixtures/rules/no-v-model-nonreactive-source/invalid/`
- Valid: `fixtures/rules/no-v-model-nonreactive-source/valid/`
