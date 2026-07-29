# `vue-vet/reactivity/prefer-explicit-sources-for-conditional-deps`

Category: reactivity  
Default severity: warning  
Confidence: high

Conditional reactive reads inside effects are clearer with explicit `watch` sources.

## Bad

```vue
<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const enabled = ref(false)
const result = ref(0)
watchEffect(() => {
  if (!enabled.value) return
  console.log(result.value)
})
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref, watch } from 'vue'
const enabled = ref(false)
const result = ref(0)
watch([enabled, result], () => {
  if (!enabled.value) return
  console.log(result.value)
})
</script>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

List every dependency in `watch([...])`.

## Fixtures

- Invalid: `fixtures/rules/prefer-explicit-sources-for-conditional-deps/invalid/`
- Valid: `fixtures/rules/prefer-explicit-sources-for-conditional-deps/valid/`
