# `vue-vet/correctness/no-use-css-vars-after-await`

Category: correctness
Default severity: warning
Confidence: high

**Retired.** This ID stays registered for config compatibility and never reports.

Vue 3.5 `<script setup>` restores instance context across top-level `await` (`withAsyncContext`). Registering `useCssVars` after `await` still binds to this instance.

Quiet regression (must not report):

```vue
<script setup lang="ts">
import { useCssVars } from 'vue'
await Promise.resolve()
useCssVars(() => ({}))

</script>
```

## Fixtures

- `fixtures/rules/no-use-css-vars-after-await/valid/former-invalid-basic.vue`
- `fixtures/rules/no-use-css-vars-after-await/valid/safe.vue`
