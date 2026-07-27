# Prefer VueUse `useMutationObserver` when observers lack disconnect

Default severity: **info**. Category: **practice** (excluded from score and default CI exit).

Constructing `MutationObserver` inside a setup lifecycle hook (`onMounted` / `onBeforeMount` / `onActivated`) without `disconnect` often leaks after unmount. VueUse `useMutationObserver` pairs the observer with automatic cleanup.

## Bad

```vue
<script setup>
import { onMounted, ref } from 'vue'

const el = ref(null)

onMounted(() => {
  const observer = new MutationObserver(() => {
    // mutated
  })
  if (el.value) {
    observer.observe(el.value, { childList: true })
  }
})
</script>
```

## Good

```vue
<script setup>
import { ref } from 'vue'
import { useMutationObserver } from '@vueuse/core'

const el = ref(null)

useMutationObserver(el, () => {
  // mutated
}, { childList: true })
</script>
```

## Limitations

Reports only when a setup lifecycle hook and `new MutationObserver(...)` appear in the same script block with no `disconnect` call (including `observer.disconnect`). Module-level constructors, explicit disconnect pairs, and already importing or calling `useMutationObserver` stay quiet. Test files are skipped.

## Remediation

Optional dependency: install `@vueuse/core` when you want the helper, then replace the manual observer with `useMutationObserver`.
