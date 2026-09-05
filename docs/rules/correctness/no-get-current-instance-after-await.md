# `vue-vet/correctness/no-get-current-instance-after-await`

Category: correctness
Default severity: warning
Confidence: high

**Retired.** This ID stays registered for config compatibility and never reports.

Vue 3.5 `<script setup>` restores instance context across top-level `await` (`withAsyncContext`). Registering `getCurrentInstance` after `await` still binds to this instance.

Quiet regression (must not report):

```vue
<script setup lang="ts">
import { getCurrentInstance } from 'vue'
await Promise.resolve()
getCurrentInstance()

</script>
```

## Fixtures

- `fixtures/rules/no-get-current-instance-after-await/valid/former-invalid-basic.vue`
- `fixtures/rules/no-get-current-instance-after-await/valid/safe.vue`
