# `vue-vet/reactivity/no-route-destructure`

Category: **practice** (excluded from score and default CI exit)

Default severity: info

Confidence: high

The stable rule id remains `vue-vet/reactivity/no-route-destructure`.

Destructuring `useRoute()` snapshots route fields. That is a live-reactivity
loss if the values are consumed across navigations, and a valid initialization
snapshot when the fields are read once. The finding stays in the practice
channel, which is excluded from scoring.

## Bad

```vue
<script setup lang="ts">
import { reactive } from 'vue'
const { count } = useRoute()
</script>

<template>
  <p>{{ count }}</p>
</template>
```

## Good

```vue
<script setup lang="ts">
import { toRefs } from 'vue'
// Keep the reactive object and read through it, or use toRefs / storeToRefs.
const state = /* useRoute */ ({ count: 0 } as any)
const { count } = toRefs(state)
</script>

<template>
  <p>{{ count }}</p>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Keep the route object or read `route.params` / `route.query` through it.

## Fixtures

- Invalid: `fixtures/rules/no-route-destructure/invalid/`
- Valid: `fixtures/rules/no-route-destructure/valid/`
