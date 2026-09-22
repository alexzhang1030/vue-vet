# `vue-vet/reactivity/no-model-default-unsynced-parent-demand`

Category: reactivity  
Default severity: warning  
Confidence: high

A child `defineModel` default applies locally when the parent v-model source is `undefined`. Vue 3.5 does not write that default back. A later parent capability demand then fails while the child value would succeed.

## Bad

```vue
<!-- Parent.vue -->
<script setup>
import { ref, onMounted } from 'vue'
import Child from './Child.vue'
const value = ref()
onMounted(() => {
  value.value.toFixed(2)
})
</script>
<template>
  <Child v-model="value" />
</template>
```

```vue
<!-- Child.vue -->
<script setup>
const model = defineModel({ default: 1 })
</script>
<template>
  <div>{{ model }}</div>
</template>
```

The same join covers kebab-case tags resolved through the project component graph (`<my-child v-model="value" />` after `import MyChild from './MyChild.vue'`).

## Good

```vue
<script setup>
import { ref, onMounted } from 'vue'
import Child from './Child.vue'
const value = ref(1)
onMounted(() => {
  value.value.toFixed(2)
})
</script>
<template>
  <Child v-model="value" />
</template>
```

Initialize the parent, or treat `undefined` as optional (`value.value?.toFixed(2)` or `if (value.value === undefined) return` before the demand).

A child that writes a *different* literal (or a non-literal) to the model binding emits `update:modelValue` and initializes the parent. Assigning the unchanged default value does not emit.

## Detection

Project join of an unconditional static child instance (component-ness comes from the `ComponentUsage` / `AutoComponent` edge, not the tag-case heuristic), a matching `v-model` name, a literal primitive child default, a parent ordinary ref that is currently `undefined`, and a later `onMounted` native member demand. The default kind supplies the method; parent `undefined` does not.

Parent `null`, already-defined values, guarded/optional demands (including a preceding same-callback early return on the demanded binding), child-only local defaults, a changed child setter that emits, missing listeners, dynamic/conditional/async/slot children (implicit default-slot content and `<template #default>` alike), getters/setters, and unknown setup effects stay quiet. Assigning the unchanged child default does not emit in Vue 3.5.40.

`<script setup>` parents are in scope. Options API parents (`components: { Child }` plus `setup()` that returns the ref) stay quiet: the return object is an escape and this owner does not model that surface.

`no-stale-prop-flow` owns source provenance. This owner requires actual default application and a failed parent demand.

## Evidence

Vue 3.5.40 compiled-SFC premises live in `crates/vue_vet_reactivity/oracle/model-demand.mjs` (`just oracle-all`). That oracle compiles every project and rule fixture with `@vue/compiler-sfc` 3.5.40 and mounts the shipped parent/child pairs.

## Remediation

Give the parent an initial value, or validate the optional parent value before using it as the default kind.

## Fixtures

- Invalid: `fixtures/rules/no-model-default-unsynced-parent-demand/invalid/`
- Valid: `fixtures/rules/no-model-default-unsynced-parent-demand/valid/`
- Project: `fixtures/projects/model-demand/`
- Snapshots: `fixtures/snapshots/no-model-default-unsynced-parent-demand/`
