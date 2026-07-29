# `vue-vet/correctness/no-on-mounted-after-await`

Category: correctness  
Default severity: warning  
Confidence: high

In `<script setup>`, calling `onMounted` after a top-level `await` runs outside the synchronous setup instance context, so the API will not bind correctly.

## Bad

```vue
<script setup lang="ts">
import { onMounted } from 'vue'
await Promise.resolve()
onMounted(() => {})
</script>
```

## Good

```vue
<script setup lang="ts">
import { onMounted } from 'vue'
onMounted(() => {})
await Promise.resolve()
</script>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Move `onMounted` before the first top-level `await`.

## Fixtures

- Invalid: `fixtures/rules/no-on-mounted-after-await/invalid/`
- Valid: `fixtures/rules/no-on-mounted-after-await/valid/`
