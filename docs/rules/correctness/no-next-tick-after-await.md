# `vue-vet/correctness/no-next-tick-after-await`

Category: correctness
Default severity: warning
Confidence: high

**Retired.** This ID stays registered for config compatibility and never reports.

Vue 3.5 `<script setup>` restores instance context across top-level `await` (`withAsyncContext`). Registering `nextTick` after `await` still binds to this instance.

Quiet regression (must not report):

```vue
<script setup lang="ts">
import { nextTick } from 'vue'
await Promise.resolve()
nextTick(() => {})

</script>
```

## Fixtures

- `fixtures/rules/no-next-tick-after-await/valid/former-invalid-basic.vue`
- `fixtures/rules/no-next-tick-after-await/valid/safe.vue`
