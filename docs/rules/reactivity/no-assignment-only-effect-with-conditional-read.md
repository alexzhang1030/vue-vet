# `vue-vet/reactivity/no-assignment-only-effect-with-conditional-read`

Category: reactivity  
Default severity: warning  
Confidence: high

An assignment-only `watchEffect` that also has conditional reactive reads is
hard to reason about: some dependencies are gated, yet the effect's job is just
to sync a write. Prefer an explicit `watch` with listed sources.

## Bad

```vue
<script setup lang="ts">
import { ref, watchEffect } from 'vue'

const enabled = ref(false)
const source = ref(0)
const out = ref(0)

watchEffect(() => {
  if (!enabled.value) return
  out.value = source.value
})
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref, watch } from 'vue'

const enabled = ref(false)
const source = ref(0)
const out = ref(0)

watch([enabled, source], () => {
  if (!enabled.value) return
  out.value = source.value
})
</script>
```

## Detection

Fact-driven via tracking-scope facts (`assignment_only` + conditional reads on
effect-family scopes).

## Remediation

Use `watch([...])` with every dependency listed, or keep the effect's reactive
reads unconditional when an effect is truly the right tool.
