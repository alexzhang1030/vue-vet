# `vue-vet/correctness/no-use-css-module-after-await`

Category: correctness
Default severity: warning
Confidence: high

**Retired.** This ID stays registered for config compatibility and never reports.

Vue 3.5 `<script setup>` restores instance context across top-level `await` (`withAsyncContext`). Registering `useCssModule` after `await` still binds to this instance.

Quiet regression (must not report):

```vue
<script setup lang="ts">
import { useCssModule } from 'vue'
await Promise.resolve()
useCssModule()

</script>
```

## Fixtures

- `fixtures/rules/no-use-css-module-after-await/valid/former-invalid-basic.vue`
- `fixtures/rules/no-use-css-module-after-await/valid/safe.vue`
