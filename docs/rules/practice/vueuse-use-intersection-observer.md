# Prefer VueUse `useIntersectionObserver` when observers lack disconnect

Default severity: **info**. Category: **practice** (excluded from score and default CI exit).

Constructing `IntersectionObserver` inside a setup lifecycle hook (`onMounted` / `onBeforeMount` / `onActivated`) without `disconnect` often leaks after unmount. VueUse `useIntersectionObserver` pairs the observer with automatic cleanup.

## Bad

```vue
<script setup>
import { onMounted, ref } from 'vue'

const el = ref(null)

onMounted(() => {
  const observer = new IntersectionObserver(() => {
    // visible
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
import { useIntersectionObserver } from '@vueuse/core'

const el = ref(null)

useIntersectionObserver(el, () => {
  // visible
})
</script>
```

## Limitations

Reports only when a setup lifecycle hook and `new IntersectionObserver(...)` appear in the same script block with no `disconnect` call (including `observer.disconnect`). Module-level constructors, explicit disconnect pairs, and already importing or calling `useIntersectionObserver` stay quiet. Test files are skipped.

## Remediation

Optional dependency: install `@vueuse/core` when you want the helper, then replace the manual observer with `useIntersectionObserver`.
