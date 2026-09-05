# `vue-vet/correctness/no-on-unmounted-after-await`

Category: correctness
Default severity: warning
Confidence: high

**Retired.** This ID stays registered for config compatibility and never reports.

Vue 3.5 `<script setup>` restores instance context across top-level `await` (`withAsyncContext`). Registering `onUnmounted` after `await` still binds to this instance.

Quiet regression (must not report):

```vue
<script setup lang="ts">
import { onUnmounted } from 'vue'
await Promise.resolve()
onUnmounted(() => {})

</script>
```

## Fixtures

- `fixtures/rules/no-on-unmounted-after-await/valid/former-invalid-placeholder.vue`
- `fixtures/rules/no-on-unmounted-after-await/valid/safe.vue`
