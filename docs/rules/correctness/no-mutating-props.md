# `vue-vet/correctness/no-mutating-props`

Category: correctness  
Default severity: warning  
Confidence: high

Mutating a value returned from `defineProps()` fights Vue's one-way data flow.
Parent state should change through events or `v-model`, not by writing through
the props object.

## Bad

```vue
<script setup lang="ts">
const componentProps = defineProps<{ count: number }>()
componentProps.count += 1
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref } from 'vue'

const props = defineProps<{ count: number }>()
const localCount = ref(props.count)
</script>
```

Or emit an update to the parent instead of mutating the prop in place.

## Limitations

The rule follows identifiers directly assigned from `defineProps()`. Destructured
prop bindings are not treated as a mutable props object.

## Remediation

Copy the value into component-owned state, or emit an event / use `defineModel`
so the parent owns the write.
