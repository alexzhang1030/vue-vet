# `vue-vet/reactivity/no-readonly-mutation`

Category: reactivity  
Default severity: warning  
Confidence: high

Readonly projections must not be mutated.

## Bad

```vue
<script setup lang="ts">
import { reactive, readonly } from 'vue'
const state = reactive({ count: 0 })
const view = readonly(state)
view.count++
</script>
```

## Good

```vue
<script setup lang="ts">
import { reactive, readonly } from 'vue'
const state = reactive({ count: 0 })
const view = readonly(state)
state.count++
void view
</script>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Mutate the source reactive state instead.

## Fixtures

- Invalid: `fixtures/rules/no-readonly-mutation/invalid/`
- Valid: `fixtures/rules/no-readonly-mutation/valid/`
