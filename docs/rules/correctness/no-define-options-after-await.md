# `vue-vet/correctness/no-define-options-after-await`

Category: correctness
Default severity: warning
Confidence: high

**Retired.** This ID stays registered for config compatibility and never reports.

`defineOptions` is a compiler macro hoisted by `compileScript`. Source position after top-level `await` is not a runtime defect.

Quiet regression (must not report):

```vue
<script setup lang="ts">
await Promise.resolve()
defineOptions({ name: 'Quiet' })

</script>
```

## Fixtures

- `fixtures/rules/no-define-options-after-await/valid/former-invalid-basic.vue`
- `fixtures/rules/no-define-options-after-await/valid/safe.vue`
