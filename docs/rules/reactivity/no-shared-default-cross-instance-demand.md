# `vue-vet/reactivity/no-shared-default-cross-instance-demand`

Category: reactivity  
Default severity: warning  
Confidence: high

Two live component instances can share one mutable `defineModel` default. A write through instance A then makes a capability demand through instance B fail.

## Bad

Two shipped shapes share identity at runtime. Both compile under `@vue/compiler-sfc` 3.5.40.

Module `<script>` object referenced from `<script setup>`:

```vue
<!-- Child.vue -->
<script>
const shared = { n: 1 }
</script>
<script setup>
const model = defineModel({ default: () => shared })
defineExpose({ model })
</script>
<template>
  <div>{{ model.n }}</div>
</template>
```

Object or array *literal* default (Vue reuses the literal across instances):

```vue
<script setup>
const model = defineModel({ default: { n: 1 } })
defineExpose({ model })
</script>
```

```vue
<!-- Parent.vue -->
<script setup>
import { ref, onMounted } from 'vue'
import Child from './Child.vue'
const left = ref(null)
const right = ref(null)
onMounted(() => {
  left.value.model.n = 'text'
  right.value.model.n.toFixed(2)
})
</script>
<template>
  <Child ref="left" />
  <Child ref="right" />
</template>
```

`const shared = { n: 1 }` declared *inside* `<script setup>` and then used as `default: () => shared` is rejected by `@vue/compiler-sfc` (`defineModel()` cannot reference locally declared variables) and is not a positive for this rule.

## Good

```vue
<script setup>
const model = defineModel({ default: () => ({ n: 1 }) })
</script>
```

A fresh factory keeps each instance's own object. Intentional same-type shared services that are only read stay quiet.

## Detection

Two statically coexisting synchronous instances that are *not* parent-fed for that model name, a proven shared mutable own-data default (module-owned factory result, or a reused object/array literal), resolved instance results (`defineExpose` must publish the path head on a closed `<script setup>` instance), an ordered write through A that changes one primitive path, and a native demand through B that the changed kind lacks.

One-instance mutation, read-only or same-type sharing, fresh factories, supplied parent `v-model` values, mutually exclusive instances, dynamic lists, getters, transforms, cloned/unknown helpers, unknown ordering, and closed instances without `defineExpose` stay quiet. Literal or factory syntax alone is not a finding.

`<script setup>` parents are in scope. Options API parents stay quiet.

## Evidence

Vue 3.5.40 compiled-SFC premises live in `crates/vue_vet_reactivity/oracle/model-demand.mjs` (`just oracle-model-demand`). The oracle compiles the module-`<script>` shared object and the object-literal default, mounts two instances, and asserts shared identity plus the sibling demand throw.

## Remediation

Return a fresh object from the default factory when instances must not alias each other.

## Fixtures

- Invalid: `fixtures/rules/no-shared-default-cross-instance-demand/invalid/`
- Valid: `fixtures/rules/no-shared-default-cross-instance-demand/valid/`
- Project: `fixtures/projects/model-demand/`
- Snapshots: `fixtures/snapshots/no-shared-default-cross-instance-demand/`
