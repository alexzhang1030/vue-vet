# `vue-vet/correctness/no-define-props-after-await`

Category: correctness
Default severity: warning
Confidence: high

**Retired.** This ID stays registered for config compatibility and never reports.

`defineProps` is a compiler macro hoisted by `compileScript`. Source position after top-level `await` is not a runtime defect.

Quiet regression (must not report):

```vue
<script setup lang="ts">
await Promise.resolve()
defineProps<{ n: number }>()

</script>
```

## Fixtures

- `fixtures/rules/no-define-props-after-await/valid/former-invalid-basic.vue`
- `fixtures/rules/no-define-props-after-await/valid/safe.vue`
