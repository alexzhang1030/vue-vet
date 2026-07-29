# `vue-vet/reactivity/no-empty-watch-sources`

Category: reactivity  
Default severity: warning  
Confidence: high

`watch` with an empty source list never runs usefully.

## Bad

```vue
<script setup lang="ts">
import { watch } from 'vue'
watch([], () => {})
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref, watch } from 'vue'
const count = ref(0)
watch(count, () => {})
</script>
```

Composable object bags (including `.d.ts` `{ width: Ref; height: Ref }` shapes
such as VueUse `useElementSize`) seed destructured locals, so renamed sources
stay known:

```vue
<script setup lang="ts">
import { watch, type Ref } from 'vue'
declare function useElementSize(): {
  width: Ref<number>
  height: Ref<number>
}
const { width: hostWidth, height: hostHeight } = useElementSize()
watch([hostWidth, hostHeight], () => {})
</script>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Pass at least one source.

## Fixtures

- Invalid: `fixtures/rules/no-empty-watch-sources/invalid/`
- Valid: `fixtures/rules/no-empty-watch-sources/valid/`
