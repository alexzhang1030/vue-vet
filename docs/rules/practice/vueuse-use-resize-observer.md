# Prefer VueUse `useResizeObserver` when observers lack disconnect

Default severity: **info**. Category: **practice** (excluded from score and default CI exit).

Constructing `ResizeObserver` inside a setup lifecycle hook (`onMounted` / `onBeforeMount` / `onActivated`) without `disconnect` often leaks after unmount. VueUse `useResizeObserver` pairs the observer with automatic cleanup.

## Bad

```vue
<script setup>
import { onMounted, ref } from 'vue'

const el = ref(null)

onMounted(() => {
  const observer = new ResizeObserver(() => {
    // size
  })
  if (el.value) {
    observer.observe(el.value)
  }
})
</script>
```

## Good

```vue
<script setup>
import { ref } from 'vue'
import { useResizeObserver } from '@vueuse/core'

const el = ref(null)

useResizeObserver(el, () => {
  // size
})
</script>
```

## Limitations

Reports only when a setup lifecycle hook and `new ResizeObserver(...)` appear in the same script block with no `disconnect` call (including `observer.disconnect`). Module-level constructors, explicit disconnect pairs, and already importing or calling `useResizeObserver` stay quiet. Test files are skipped.

## Remediation

Optional dependency: install `@vueuse/core` when you want the helper, then replace the manual observer with `useResizeObserver`.
