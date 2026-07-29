# Prefer VueUse `useWindowSize` for hand-rolled resize tracking

Default severity: **info**. Category: **practice** (excluded from score and default CI exit).

Tracking `width`/`height` refs with a manual `resize` listener inside a setup lifecycle hook reimplements VueUse `useWindowSize`, which returns reactive `width`/`height` refs and cleans up its listener automatically.

## Bad

```vue
<script setup>
import { onMounted, ref } from 'vue'

const width = ref(window.innerWidth)
const height = ref(window.innerHeight)

onMounted(() => {
  window.addEventListener('resize', () => {
    width.value = window.innerWidth
    height.value = window.innerHeight
  })
})
</script>
```

## Good

```vue
<script setup>
import { useWindowSize } from '@vueuse/core'

const { width, height } = useWindowSize()
</script>
```

## Limitations

Fires only when a block has a setup lifecycle hook, an `addEventListener` call, and reactivity-graph `ref`/`shallowRef` bindings whose names contain `width` and `height`. Differently named size refs, or resize tracking split across multiple blocks, stay quiet.

## Remediation

Optional dependency: install `@vueuse/core` when you want the helper, then replace the manual listener with `useWindowSize()`.
