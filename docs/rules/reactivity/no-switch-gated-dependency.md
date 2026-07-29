# `vue-vet/reactivity/no-switch-gated-dependency`

Category: reactivity  
Default severity: warning  
Confidence: high  
Tier: tracer

Reactive reads inside a `switch` case are gated by the discriminant. Other cases
do not collect those dependencies.

## Bad

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'

const mode = ref('a')
const count = ref(0)
const label = computed(() => {
  switch (mode.value) {
    case 'a':
      return String(count.value)
    default:
      return 'other'
  }
})
</script>
```

## Good

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'

const mode = ref('a')
const count = ref(0)
const label = computed(() => {
  const n = count.value
  switch (mode.value) {
    case 'a':
      return String(n)
    default:
      return 'other'
  }
})
</script>
```

## Detection

Fact-driven: `Conditional` reads with `ReactiveGuardRole::SwitchDiscriminant`.

## Remediation

Read shared dependencies before the `switch`, or watch them explicitly.
