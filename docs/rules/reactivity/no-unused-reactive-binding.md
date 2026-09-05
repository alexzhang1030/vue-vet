# No unused reactive binding

A `ref`, `computed`, `reactive`, or similar local reactive binding that is never
read in script **or** template is dead state. Vue Vet joins template expression
surfaces onto the reactivity graph, so template-only usage (mustache, `v-if`,
`v-for` source, `v-bind`, …) counts as a use.

## Bad

```vue
<script setup>
import { ref } from 'vue'
const orphan = ref(0)
</script>

<template>
  <div />
</template>
```

## Good

```vue
<script setup>
import { ref } from 'vue'
const count = ref(0)
</script>

<template>
  <div>{{ count }}</div>
</template>
```

```vue
<script setup>
import { ref, watchEffect } from 'vue'
const count = ref(0)
watchEffect(() => {
  console.log(count.value)
})
</script>
```

## Detection

For each reactive binding fact the rule checks:

- script symbol reads (`ScriptBindingFact.reads`)
- reactivity-scope reads and writes
- inverted dependency edges
- template→script joins (`template_reads`)
- static `ref="name"` attributes (template ref string form)

Quiet for framework contracts that are often consumed without a local read:
`defineModel`, `useTemplateRef`, and `toRef` / `toRefs` seeds.
Exported module APIs (`export const count = ref(0)`, `export { count }`,
composables that return a ref, ordinary `<script>` exports) are not unused
locals. Same-name inner bindings are matched by declaration span so an exported
outer `count` does not hide an inner unused `count`.

## Remediation

Delete the unused binding, or wire it into a template expression, tracking
scope, or other script read.

## Fixtures

- Invalid: `fixtures/rules/no-unused-reactive-binding/invalid/` (`orphan-ref.vue`)
- Valid: `fixtures/rules/no-unused-reactive-binding/valid/`
  (`script-read.vue`, `template-read.vue`, `inner-then-template.vue`,
  `ordinary-script-export.vue`)
