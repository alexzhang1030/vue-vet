# `vue-vet/correctness/no-inject-after-await`

Category: correctness
Default severity: warning
Confidence: high

**Retired.** This ID stays registered for config compatibility and never reports.

Vue 3.5 `<script setup>` restores instance context across top-level `await` (`withAsyncContext`). Registering `inject` after `await` still binds to this instance.

Quiet regression (must not report):

```vue
<script setup lang="ts">
import { inject } from 'vue'
await Promise.resolve()
inject('k')

</script>
```

## Fixtures

- `fixtures/rules/no-inject-after-await/valid/former-invalid-placeholder.vue`
- `fixtures/rules/no-inject-after-await/valid/safe.vue`
