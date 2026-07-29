# `vue-vet/reactivity/no-watch-callback-as-tracking-scope`

Category: reactivity  
Default severity: warning  
Confidence: high

`watch` callbacks are not tracking scopes. Reactive reads there do not subscribe like `watchEffect`.

## Bad

```vue
<script setup lang="ts">
import { ref, watch } from 'vue'
const a = ref(0)
const b = ref(0)
watch(a, () => {
  console.log(b.value)
})
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref, watch } from 'vue'
const a = ref(0)
const b = ref(0)
watch([a, b], () => {
  console.log(b.value)
})
</script>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

List every value you need in the watch source list.

## Fixtures

- Invalid: `fixtures/rules/no-watch-callback-as-tracking-scope/invalid/`
- Valid: `fixtures/rules/no-watch-callback-as-tracking-scope/valid/`
