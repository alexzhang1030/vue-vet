# `vue-vet/reactivity/no-conditional-dependency-in-computed`

Category: reactivity
Default severity: warning
Confidence: high

**Retired.** This ID stays registered for config compatibility and never reports.

Vue tracks dynamic dependencies: `enabled.value ? count.value : 0` re-runs when `enabled` changes and then picks up `count`. A reactive guard is valid tracking.

Quiet regression (must not report):

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'
const enabled = ref(false)
const count = ref(0)
function load() {
  return String(count.value)
}
const label = computed(() => (enabled.value ? load() : 'off'))
</script>
<template>
  <p>{{ label }}</p>
</template>
```

## Fixtures

- `fixtures/rules/no-conditional-dependency-in-computed/valid/both-arms-helper.vue`
- `fixtures/rules/no-conditional-dependency-in-computed/valid/former-invalid-helper-ternary.vue`
- `fixtures/rules/no-conditional-dependency-in-computed/valid/former-invalid-inline-ternary.vue`
- `fixtures/rules/no-conditional-dependency-in-computed/valid/former-invalid-placeholder.vue`
- `fixtures/rules/no-conditional-dependency-in-computed/valid/safe.vue`
- `fixtures/rules/no-conditional-dependency-in-computed/valid/unconditional-helper.vue`
