# `vue-vet/correctness/no-use-slots-after-await`

Category: correctness
Default severity: warning
Confidence: high

**Retired.** This ID stays registered for config compatibility and never reports.

Vue 3.5 `<script setup>` restores instance context across top-level `await` (`withAsyncContext`). Registering `useSlots` after `await` still binds to this instance.

Quiet regression (must not report):

```vue
<script setup lang="ts">
import { useSlots } from 'vue'
await Promise.resolve()
useSlots()

</script>
```

## Fixtures

- `fixtures/rules/no-use-slots-after-await/valid/former-invalid-basic.vue`
- `fixtures/rules/no-use-slots-after-await/valid/safe.vue`
