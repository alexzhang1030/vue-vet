# `vue-vet/correctness/no-effect-scope-after-await`

Category: correctness
Default severity: warning
Confidence: high

**Retired.** This ID stays registered for config compatibility and never reports.

Vue 3.5 `<script setup>` restores instance context across top-level `await` (`withAsyncContext`). Registering `effectScope` after `await` still binds to this instance.

Quiet regression (must not report):

```vue
<script setup lang="ts">
import { effectScope } from 'vue'
await Promise.resolve()
effectScope()

</script>
```

## Fixtures

- `fixtures/rules/no-effect-scope-after-await/valid/former-invalid-basic.vue`
- `fixtures/rules/no-effect-scope-after-await/valid/safe.vue`
