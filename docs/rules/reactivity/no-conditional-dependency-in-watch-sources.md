# `vue-vet/reactivity/no-conditional-dependency-in-watch-sources`

Category: reactivity
Default severity: warning
Confidence: high

**Retired.** This ID stays registered for config compatibility and never reports.

Vue tracks dynamic dependencies. A reactive guard still subscribes; later reads are picked up on the next run. That is valid tracking, not a defect.

Quiet regression (must not report):

```vue
<script setup lang="ts">
import { ref, watch } from 'vue'
const enabled = ref(false)
const count = ref(0)
watch(
  () => (enabled.value ? count.value : 0),
  () => {},
)
</script>
```

## Fixtures

- `fixtures/rules/no-conditional-dependency-in-watch-sources/valid/former-invalid-placeholder.vue`
- `fixtures/rules/no-conditional-dependency-in-watch-sources/valid/safe.vue`
