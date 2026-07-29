# `vue-vet/reactivity/prefer-watch-over-effect-for-single-source`

Category: reactivity  
Default severity: warning  
Confidence: high

An assignment-only `watchEffect` that tracks a single unconditional source is clearer as `watch`.

## Bad

```vue
<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const source = ref(0)
const out = ref(0)
watchEffect(() => {
  out.value = source.value
})
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref, watch } from 'vue'
const source = ref(0)
const out = ref(0)
watch(source, (value) => {
  out.value = value
})
</script>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Prefer `watch(source, ...)` for single-source sync.

## Fixtures

- Invalid: `fixtures/rules/prefer-watch-over-effect-for-single-source/invalid/`
- Valid: `fixtures/rules/prefer-watch-over-effect-for-single-source/valid/`
