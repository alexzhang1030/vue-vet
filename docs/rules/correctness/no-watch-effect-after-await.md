# `vue-vet/correctness/no-watch-effect-after-await`

Category: correctness
Default severity: warning
Confidence: high

**Retired.** This ID stays registered for config compatibility and never reports.

Vue 3.5 `<script setup>` restores instance context across top-level `await` (`withAsyncContext`). Registering `watchEffect` after `await` still binds to this instance.

Quiet regression (must not report):

```vue
<script setup lang="ts">
import { watchEffect } from 'vue'
await Promise.resolve()
watchEffect(() => {})

</script>
```

## Fixtures

- `fixtures/rules/no-watch-effect-after-await/valid/former-invalid-placeholder.vue`
- `fixtures/rules/no-watch-effect-after-await/valid/safe.vue`
