# `vue-vet/reactivity/no-computed-as-operand`

Category: reactivity  
Default severity: warning  
Confidence: high

Using a computed ref object directly as an operand reads the object wrapper, not the inner value. Unwrap with `.value` (or `toValue`).

## Bad

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const doubled = computed(() => count.value * 2)
const ok = doubled > 0
</script>

<template>
  <p>{{ ok }}</p>
</template>
```

## Good

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const doubled = computed(() => count.value * 2)
const ok = doubled.value > 0
</script>

<template>
  <p>{{ ok }}</p>
</template>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).
Shares the operand matcher with `no-ref-as-operand`: identifiers match a
computed binding by Oxc declaration span (`ScriptOperandFact.binding_span`),
not by name. A watch callback parameter, destructured watch argument, nested
local, or same-name binding in another function does not inherit the outer
computed. Unresolved identifiers (bare auto-imported exported computeds) match
a unique proven seed only when this module has no local symbol of that name.

## Remediation

Read `computed ref.value` (or `toValue(...)`) at the use site.

## Fixtures

- Invalid: `fixtures/rules/no-computed-as-operand/invalid/` (`basic.vue`)
- Valid: `fixtures/rules/no-computed-as-operand/valid/`
  (`safe.vue`, `watch-callback-shadow.vue`)
