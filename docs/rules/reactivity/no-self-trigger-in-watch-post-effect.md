# `vue-vet/reactivity/no-self-trigger-in-watch-post-effect`

Category: reactivity  
Default severity: warning  
Confidence: high

`watchEffect` that writes a dependency it also reads can loop.

## Bad

```vue
<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const count = ref(0)
watchEffect(() => {
  count.value = count.value + 1
})
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const count = ref(0)
const doubled = ref(0)
watchEffect(() => {
  doubled.value = count.value * 2
})
</script>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Write a different binding, or use `watch` with explicit sources.

## Fixtures

- Invalid: `fixtures/rules/no-self-trigger-in-watch-post-effect/invalid/`
- Valid: `fixtures/rules/no-self-trigger-in-watch-post-effect/valid/`
