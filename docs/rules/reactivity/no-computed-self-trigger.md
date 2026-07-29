# `vue-vet/reactivity/no-computed-self-trigger`

Category: reactivity  
Default severity: warning  
Confidence: high

`computed` that writes a dependency it reads can self-trigger.

## Bad

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const label = computed(() => {
  count.value++
  return count.value
})
</script>
```

## Good

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const label = computed(() => count.value)
</script>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Keep computed getters pure.

## Fixtures

- Invalid: `fixtures/rules/no-computed-self-trigger/invalid/`
- Valid: `fixtures/rules/no-computed-self-trigger/valid/`
