# Prefer VueUse `useTimeoutFn` when timeouts lack cleanup

Default severity: **info**. Category: **practice** (excluded from score and default CI exit).

Evidence is **lexical enclosing-call ancestors**, not the later invocation site. `setTimeout` nested under a setup lifecycle hook (`onMounted` / `onBeforeMount` / `onActivated`) without `clearTimeout` is reported even when the timer is registered inside a nested subscriber/`afterEach` callback that later runs outside setup. VueUse `useTimeoutFn` should be **constructed during setup** and started from that callback so unmount can cancel it.

## Bad

```vue
<script setup>
import { onMounted } from 'vue'

onMounted(() => {
  setTimeout(() => {
    console.log('ready')
  }, 1000)
})
</script>
```

## Good

```vue
<script setup>
import { useTimeoutFn } from '@vueuse/core'

useTimeoutFn(() => {
  console.log('ready')
}, 1000)
</script>
```

## Limitations

Reports only when `setTimeout` has a setup lifecycle hook in its lexical enclosing-call ancestors, with no `clearTimeout`. A timeout inside `watch` / `watchEffect` is not a lifecycle match merely because `onMounted` exists elsewhere in the same block. Nested subscribe/`afterEach` callbacks that sit under `onMounted` still match; the recipe does not analyze callback invocation time. Module-level timeouts and clear+set debounce patterns stay quiet (the latter may be suggested as `useDebounceFn` instead). Already importing or calling `useTimeoutFn` is a safe pattern. Test files are skipped.

## Remediation

Optional dependency: install `@vueuse/core`. Create `useTimeoutFn` during setup with `{ immediate: false }`, then call `start()` from the callback instead of a bare `setTimeout`.
