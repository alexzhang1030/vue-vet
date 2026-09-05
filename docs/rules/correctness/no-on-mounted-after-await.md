# `vue-vet/correctness/no-on-mounted-after-await`

Category: correctness
Default severity: warning
Confidence: high

**Retired.** This ID stays registered for config compatibility and never reports.

Vue 3.5 `<script setup>` restores instance context across top-level `await` (`withAsyncContext`). Registering `onMounted` after `await` still binds to this instance.

Still-live related rules: `no-define-expose-after-await` for expose timing, and `no-after-await-watch-effect-dependency` for reactive reads after `await` inside an effect.

Quiet regression (must not report):

```vue
<script setup lang="ts">
import { onMounted } from 'vue'
await Promise.resolve()
onMounted(() => {})
</script>
```

## Fixtures

- `fixtures/rules/no-on-mounted-after-await/valid/former-invalid-basic.vue`
- `fixtures/rules/no-on-mounted-after-await/valid/safe.vue`
