# `vue-vet/correctness/no-with-defaults-after-await`

Category: correctness
Default severity: warning
Confidence: high

**Retired.** This ID stays registered for config compatibility and never reports.

`withDefaults` is a compiler macro hoisted by `compileScript`. Source position after top-level `await` is not a runtime defect.

Quiet regression (must not report):

```vue
<script setup lang="ts">
await Promise.resolve()
withDefaults(defineProps<{ n?: number }>(), { n: 0 })

</script>
```

## Fixtures

- `fixtures/rules/no-with-defaults-after-await/valid/former-invalid-basic.vue`
- `fixtures/rules/no-with-defaults-after-await/valid/safe.vue`
