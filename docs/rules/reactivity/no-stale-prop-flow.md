# `vue-vet/reactivity/no-stale-prop-flow`

Category: reactivity  
Default severity: warning  
Confidence: high

Cross-file prop edges should start from reactive parent state; plain values go stale.

## Bad

```vue
<!-- parent -->
<script setup lang="ts">
let title = 'hi'
</script>
<template>
  <Child :title="title" />
</template>
```

## Good

```vue
<!-- parent -->
<script setup lang="ts">
import { ref } from 'vue'
const title = ref('hi')
</script>
<template>
  <Child :title="title" />
</template>
```

## Detection

After the reactive prop join, the parent graph gets a Prop edge only when all of these hold:

- the child script has a reactive `props` bag
- the binding is `v-bind` / `:prop`, not `v-model` and not a whole-object `v-bind`
- the expression is a bare identifier or a static member chain
- that root is not already a reactive binding
- the root is a plain `let` or `var` (`const` cannot go stale)

The edge is `from` = that local, `to` = `props`, `property` = the prop name, span on the directive. Child edges for reactive sources stay `from: "props"` and are not a finding. Literals, calls, `props.title`, destructured `defineProps`, `toRef` / `computed`, and component `v-model` stay quiet.

## Remediation

Pass a reactive binding (ref/computed/reactive field).

## Fixtures

- Invalid: `fixtures/rules/no-stale-prop-flow/invalid/`
- Valid: `fixtures/rules/no-stale-prop-flow/valid/`
