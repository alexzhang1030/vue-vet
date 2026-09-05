# `vue-vet/reactivity/no-conditional-dependency-in-effect-scope`

Category: reactivity
Default severity: warning
Confidence: high

**Retired.** This ID stays registered for config compatibility and never reports.

Vue tracks dynamic dependencies. A reactive guard still subscribes; later reads are picked up on the next run. That is valid tracking, not a defect.

Quiet regression (must not report):

```vue
<script setup lang="ts">
import { effectScope, ref } from 'vue'
const enabled = ref(false)
const count = ref(0)
const scope = effectScope()
scope.run(() => {
  if (!enabled.value) return
  console.log(count.value)
})
</script>
```

## Fixtures

- `fixtures/rules/no-conditional-dependency-in-effect-scope/valid/former-invalid-placeholder.vue`
- `fixtures/rules/no-conditional-dependency-in-effect-scope/valid/safe.vue`
