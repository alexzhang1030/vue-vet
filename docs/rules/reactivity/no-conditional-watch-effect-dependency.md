# `vue-vet/reactivity/no-conditional-watch-effect-dependency`

Category: reactivity
Default severity: warning
Confidence: high

**Retired.** This ID stays registered for config compatibility and never reports.

Vue tracks dynamic dependencies. A reactive guard still subscribes; later reads are picked up on the next run. That is valid tracking, not a defect.

Quiet regression (must not report):

```vue
<script setup lang="ts">
import { ref, watchEffect } from 'vue'

const enabled = ref(false)
const result = ref(0)

watchEffect(() => {
  if (!enabled.value) return
  console.log(result.value)
})
</script>
```

## Fixtures

- `fixtures/rules/no-conditional-watch-effect-dependency/valid/explicit-watch.vue`
- `fixtures/rules/no-conditional-watch-effect-dependency/valid/former-invalid-guarded.vue`
- `fixtures/rules/no-conditional-watch-effect-dependency/valid/nested-callback.vue`
