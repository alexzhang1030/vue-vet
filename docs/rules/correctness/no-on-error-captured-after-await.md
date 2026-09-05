# `vue-vet/correctness/no-on-error-captured-after-await`

Category: correctness
Default severity: warning
Confidence: high

**Retired.** This ID stays registered for config compatibility and never reports.

Vue 3.5 `<script setup>` restores instance context across top-level `await` (`withAsyncContext`). Registering `onErrorCaptured` after `await` still binds to this instance.

Quiet regression (must not report):

```vue
<script setup lang="ts">
import { onErrorCaptured } from 'vue'
await Promise.resolve()
onErrorCaptured(() => {})

</script>
```

## Fixtures

- `fixtures/rules/no-on-error-captured-after-await/valid/former-invalid-placeholder.vue`
- `fixtures/rules/no-on-error-captured-after-await/valid/safe.vue`
