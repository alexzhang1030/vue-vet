# `vue-vet/reactivity/no-effect-write-without-read`

Category: reactivity  
Default severity: warning  
Confidence: high

`watchEffect` that only writes and never reads reactive state will not re-run usefully.

## Bad

```vue
<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const out = ref(0)
watchEffect(() => {
  out.value = 1
})
</script>
```

## Good

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

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Read the inputs you depend on, or use a one-shot assignment outside an effect.

## Fixtures

- Invalid: `fixtures/rules/no-effect-write-without-read/invalid/`
- Valid: `fixtures/rules/no-effect-write-without-read/valid/`
