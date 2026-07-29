# `vue-vet/reactivity/no-props-snapshot-in-ref`

Category: reactivity  
Default severity: warning  
Confidence: high

Wrapping `props` fields in `ref(props.x)` snapshots the current value and drops prop reactivity.

## Bad

```vue
<script setup lang="ts">
import { ref } from 'vue'
const props = defineProps<{ title: string }>()
const title = ref(props.title)
</script>
```

## Good

```vue
<script setup lang="ts">
import { toRef } from 'vue'
const props = defineProps<{ title: string }>()
const title = toRef(props, 'title')
</script>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Use `toRef` / `toRefs`, or read `props.title` directly.

## Fixtures

- Invalid: `fixtures/rules/no-props-snapshot-in-ref/invalid/`
- Valid: `fixtures/rules/no-props-snapshot-in-ref/valid/`
