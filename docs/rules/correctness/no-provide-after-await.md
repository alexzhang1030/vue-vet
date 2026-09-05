# `vue-vet/correctness/no-provide-after-await`

Category: correctness
Default severity: warning
Confidence: high

**Retired.** This ID stays registered for config compatibility and never reports.

Vue 3.5 `<script setup>` restores instance context across top-level `await` (`withAsyncContext`). Registering `provide` after `await` still binds to this instance.

Quiet regression (must not report):

```vue
<script setup lang="ts">
import { provide } from 'vue'
await Promise.resolve()
provide('k', 1)

</script>
```

## Fixtures

- `fixtures/rules/no-provide-after-await/valid/former-invalid-placeholder.vue`
- `fixtures/rules/no-provide-after-await/valid/safe.vue`
