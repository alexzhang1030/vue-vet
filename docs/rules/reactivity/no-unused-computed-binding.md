# `vue-vet/reactivity/no-unused-computed-binding`

Category: reactivity  
Default severity: warning  
Confidence: high

A `computed` binding that is never read is dead work.

## Bad

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const doubled = computed(() => count.value * 2)
</script>
<template>
  <p>{{ count }}</p>
</template>
```

## Good

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const doubled = computed(() => count.value * 2)
</script>
<template>
  <p>{{ doubled }}</p>
</template>
```

A CSS `v-bind(ident)` / `v-bind('ident')` on a simple identifier also counts as
a use. Complex style expressions (`v-bind("height + 'px'")`, `v-bind(theme.color)`)
stay quiet (under-approx).

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Read the computed in script/template, or delete it.

## Fixtures

- Invalid: `fixtures/rules/no-unused-computed-binding/invalid/`
- Valid: `fixtures/rules/no-unused-computed-binding/valid/`
  (`safe.vue`; `style-v-bind.vue` computed used only from CSS `v-bind`)
